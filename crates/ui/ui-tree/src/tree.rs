use std::cell::{Cell, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::{Effect, ReadSignal, RwSignal, batch, create_effect, create_rw_signal};
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::view::SubtreeHandle;
use crate::view_flatten;

struct ComponentSlot {
    component: Rc<RefCell<Box<dyn Component>>>,
    commands: Rc<RefCell<Vec<DrawCommand>>>,
    dirty: Rc<Cell<bool>>,
    _effect: Effect,
}

impl ComponentSlot {
    fn new<C: Component + 'static>(component: C) -> Self {
        let component = Rc::new(RefCell::new(Box::new(component) as Box<dyn Component>));
        let commands: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let dirty = Rc::new(Cell::new(true));

        let comp_clone = Rc::clone(&component);
        let cmds_clone = Rc::clone(&commands);
        let dirty_clone = Rc::clone(&dirty);
        let _effect = create_effect(move || {
            let view = comp_clone.borrow().view();
            let flat = view_flatten::flatten_view(view);
            let mut cmds = cmds_clone.borrow_mut();
            cmds.clear();
            cmds.extend(flat);
            dirty_clone.set(true);
        });

        ComponentSlot {
            component,
            commands,
            dirty,
            _effect,
        }
    }
}

pub struct SubtreeSlot {
    component: Rc<RefCell<Box<dyn Component>>>,
    commands: Rc<RefCell<Vec<DrawCommand>>>,
    version: RwSignal<u32>,
    _effect: Effect,
}

impl SubtreeSlot {
    pub fn new<C: Component + 'static>(component: C) -> Self {
        let component = Rc::new(RefCell::new(Box::new(component) as Box<dyn Component>));
        let commands: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let version = create_rw_signal(0u32);

        let comp_clone = Rc::clone(&component);
        let cmds_clone = Rc::clone(&commands);
        let ver_clone = version.clone();
        let _effect = create_effect(move || {
            let view = comp_clone.borrow().view();
            let flat = view_flatten::flatten_view(view);
            *cmds_clone.borrow_mut() = flat;
            ver_clone.update(|v| *v = v.wrapping_add(1));
        });

        Self {
            component,
            commands,
            version,
            _effect,
        }
    }

    pub fn handle(&self) -> SubtreeHandle {
        SubtreeHandle::new(Rc::clone(&self.commands))
    }

    pub fn version(&self) -> ReadSignal<u32> {
        self.version.read_only()
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        self.component.borrow_mut().on_event(event)
    }
}

pub struct ComponentTree {
    slots: Vec<ComponentSlot>,
    cached: RefCell<Vec<DrawCommand>>,
}

impl ComponentTree {
    pub fn new<C: Component + 'static>(component: C) -> Self {
        component.on_mount();
        Self {
            slots: vec![ComponentSlot::new(component)],
            cached: RefCell::new(Vec::new()),
        }
    }

    pub fn add<C: Component + 'static>(&mut self, component: C) {
        component.on_mount();
        self.slots.push(ComponentSlot::new(component));
    }

    pub fn commands(&self) -> Vec<DrawCommand> {
        let any_dirty = self.slots.iter().any(|s| s.dirty.get());
        if any_dirty {
            let mut cached = self.cached.borrow_mut();
            cached.clear();
            for slot in &self.slots {
                cached.extend(slot.commands.borrow().iter().cloned());
                slot.dirty.set(false);
            }
            cached.clone()
        } else {
            self.cached.borrow().clone()
        }
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

impl Drop for ComponentTree {
    fn drop(&mut self) {
        for slot in &self.slots {
            slot.component.borrow().on_unmount();
        }
    }
}

#[cfg(test)]
mod tests {
    use reactive_core::create_rw_signal;
    use renderer_core::{Color, Rect, RectStyle};

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

    struct Lifecycle {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Component for Lifecycle {
        fn view(&self) -> View {
            View::Empty
        }

        fn on_mount(&self) {
            self.log.borrow_mut().push("mount");
        }

        fn on_unmount(&self) {
            self.log.borrow_mut().push("unmount");
        }
    }

    #[test]
    fn tree_lifecycle() {
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        {
            let tree = ComponentTree::new(Lifecycle {
                log: Rc::clone(&log),
            });
            assert_eq!(*log.borrow(), vec!["mount"]);
            assert!(tree.commands().is_empty());
        }
        assert_eq!(*log.borrow(), vec!["mount", "unmount"]);
    }
}
