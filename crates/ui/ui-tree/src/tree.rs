use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::{Effect, batch, create_effect};
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::render_node::RenderNode;
use crate::view_flatten;

struct ComponentSlot {
    component: Rc<RefCell<dyn Component>>,
    commands: Rc<RefCell<Vec<DrawCommand>>>,
    dirty: Rc<Cell<bool>>,
    _stack: Rc<RefCell<Vec<RenderNode>>>,
    _effect: Effect,
}

impl ComponentSlot {
    fn new<C: Component + 'static>(component: C) -> Self {
        let component: Rc<RefCell<dyn Component>> = Rc::new(RefCell::new(component));
        let commands: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let dirty = Rc::new(Cell::new(true));
        let stack: Rc<RefCell<Vec<RenderNode>>> = Default::default();

        let comp_clone = Rc::clone(&component);
        let cmds_clone = Rc::clone(&commands);
        let dirty_clone = Rc::clone(&dirty);
        let stack_clone = Rc::clone(&stack);
        let _effect = create_effect(move || {
            let node = comp_clone.borrow().view();
            let mut stk = stack_clone.borrow_mut();
            let mut cmds = cmds_clone.borrow_mut();
            let changed = view_flatten::flatten_view(node, &mut *cmds, &mut *stk);
            if changed {
                dirty_clone.set(true);
            }
        });

        ComponentSlot {
            component,
            commands,
            dirty,
            _stack: stack,
            _effect,
        }
    }
}

pub struct ComponentTree {
    slots: Vec<ComponentSlot>,
    cached: RefCell<Vec<DrawCommand>>,
    slot_starts: RefCell<Vec<usize>>,
}

impl ComponentTree {
    pub fn new<C: Component + 'static>(component: C) -> Self {
        Self {
            slots: vec![ComponentSlot::new(component)],
            cached: RefCell::new(Vec::new()),
            slot_starts: RefCell::new(Vec::new()),
        }
    }

    pub fn add<C: Component + 'static>(&mut self, component: C) {
        self.slots.push(ComponentSlot::new(component));
        self.slot_starts.borrow_mut().clear();
    }

    pub fn is_dirty(&self) -> bool {
        self.slots.iter().any(|s| s.dirty.get())
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        // single-slot fast path: flatten_view already wrote into the slot vec in-place,
        // so we can return a ref to it directly without copying into `cached`
        if let [slot] = self.slots.as_slice() {
            slot.dirty.set(false);
            return slot.commands.borrow();
        }
        let any_dirty = self.slots.iter().any(|s| s.dirty.get());
        if any_dirty {
            let mut cached = self.cached.borrow_mut();
            let mut starts = self.slot_starts.borrow_mut();
            if starts.len() != self.slots.len() {
                // Full rebuild when slot count changed or on first use.
                cached.clear();
                starts.clear();
                for slot in &self.slots {
                    starts.push(cached.len());
                    cached.extend(slot.commands.borrow().iter().cloned());
                    slot.dirty.set(false);
                }
            } else {
                // Incremental: splice only dirty slots, leaving clean slots untouched.
                let mut offset: isize = 0;
                for (i, slot) in self.slots.iter().enumerate() {
                    if slot.dirty.get() {
                        let start = (starts[i] as isize + offset) as usize;
                        let end = if i + 1 < self.slots.len() {
                            (starts[i + 1] as isize + offset) as usize
                        } else {
                            cached.len()
                        };
                        let new_cmds: Vec<DrawCommand> =
                            slot.commands.borrow().iter().cloned().collect();
                        let delta = new_cmds.len() as isize - (end - start) as isize;
                        cached.splice(start..end, new_cmds);
                        offset += delta;
                        slot.dirty.set(false);
                    }
                }
                let mut off = 0;
                for (i, slot) in self.slots.iter().enumerate() {
                    starts[i] = off;
                    off += slot.commands.borrow().len();
                }
            }
        }
        self.cached.borrow()
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        batch(|| {
            for slot in &mut self.slots {
                let result = slot.component.borrow_mut().on_event(event);
                if result == EventResult::Handled {
                    return EventResult::Handled;
                }
            }
            EventResult::Ignored
        })
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use reactive_core::create_rw_signal;
    use renderer_core::{Color, RectPayload, RectStyle};

    use super::*;
    use crate::render_node::RenderNode;

    fn sample_rect(x: f32) -> DrawCommand {
        DrawCommand::Rect(Box::new(RectPayload {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            style: RectStyle::default().with_fill(Color::BLACK),
        }))
    }

    struct Fixed;

    impl Component for Fixed {
        fn view(&self) -> RenderNode {
            RenderNode::group([
                RenderNode::Primitive(sample_rect(0.0)),
                RenderNode::Primitive(sample_rect(20.0)),
            ])
        }
    }

    #[test]
    fn tree_initial_render() {
        let tree = ComponentTree::new(Fixed);
        let cmds = tree.commands();
        assert_eq!(cmds.len(), 2);
    }

    struct Counter {
        value: reactive_core::RwSignal<i32>,
    }

    impl Component for Counter {
        fn view(&self) -> RenderNode {
            let n = self.value.get();
            RenderNode::group((0..n).map(|i| RenderNode::Primitive(sample_rect(i as f32 * 10.0))))
        }
    }

    #[test]
    fn tree_reactive_update() {
        let signal = create_rw_signal(2i32);
        let tree = ComponentTree::new(Counter {
            value: signal.clone(),
        });

        assert_eq!(tree.commands().len(), 2);

        signal.set(5);
        assert_eq!(tree.commands().len(), 5);

        signal.set(0);
        assert_eq!(tree.commands().len(), 0);
    }
}
