use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::{Effect, batch, create_effect};
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::view::View;
use crate::view_flatten;

struct ComponentSlot {
    component: Rc<RefCell<dyn Component>>,
    commands: Rc<RefCell<Vec<DrawCommand>>>,
    dirty: Rc<Cell<bool>>,
    _stack: Rc<RefCell<Vec<View>>>,
    _effect: Effect,
}

impl ComponentSlot {
    fn new<C: Component + 'static>(component: C) -> Self {
        let component: Rc<RefCell<dyn Component>> = Rc::new(RefCell::new(component));
        let commands: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let dirty = Rc::new(Cell::new(true));
        let stack: Rc<RefCell<Vec<View>>> = Default::default();

        let comp_clone = Rc::clone(&component);
        let cmds_clone = Rc::clone(&commands);
        let dirty_clone = Rc::clone(&dirty);
        let stack_clone = Rc::clone(&stack);
        let _effect = create_effect(move || {
            let view = comp_clone.borrow().view();
            let mut stk = stack_clone.borrow_mut();
            let mut new_cmds: Vec<DrawCommand> = Vec::new();
            view_flatten::flatten_view(view, &mut new_cmds, &mut stk);
            let mut cmds = cmds_clone.borrow_mut();
            if *cmds != new_cmds {
                *cmds = new_cmds;
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
}

impl ComponentTree {
    pub fn new<C: Component + 'static>(component: C) -> Self {
        Self {
            slots: vec![ComponentSlot::new(component)],
            cached: RefCell::new(Vec::new()),
        }
    }

    pub fn add<C: Component + 'static>(&mut self, component: C) {
        self.slots.push(ComponentSlot::new(component));
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        let any_dirty = self.slots.iter().any(|s| s.dirty.get());
        if any_dirty {
            let mut cached = self.cached.borrow_mut();
            cached.clear();
            for slot in &self.slots {
                cached.extend(slot.commands.borrow().iter().cloned());
                slot.dirty.set(false);
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
    use renderer_core::{Color, RectStyle};

    use super::*;
    use crate::view::View;

    fn sample_rect(x: f32) -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            style: RectStyle::default().with_fill(Color::BLACK),
        }
    }

    struct Fixed;

    impl Component for Fixed {
        fn view(&self) -> View {
            View::Group(vec![
                View::Primitive(sample_rect(0.0)),
                View::Primitive(sample_rect(20.0)),
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
        fn view(&self) -> View {
            let n = self.value.get();
            let children: Vec<View> = (0..n)
                .map(|i| View::Primitive(sample_rect(i as f32 * 10.0)))
                .collect();
            View::Group(children)
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
