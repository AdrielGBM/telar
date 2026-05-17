use std::cell::RefCell;
use std::rc::Rc;

use platform_core::Event;
use reactive_core::{Effect, batch, create_effect};
use renderer_core::DrawCommand;

use crate::component::{AnyComponent, Component, EventResult};
use crate::reconciler;

pub struct ComponentTree {
    root: Rc<RefCell<Box<dyn AnyComponent>>>,
    commands: Rc<RefCell<Vec<DrawCommand>>>,
    _effect: Effect,
}

impl ComponentTree {
    pub fn new<C: Component>(component: C) -> Self {
        component.on_mount();
        let root: Rc<RefCell<Box<dyn AnyComponent>>> = Rc::new(RefCell::new(Box::new(component)));
        let commands: Rc<RefCell<Vec<DrawCommand>>> = Rc::new(RefCell::new(Vec::new()));

        let root_clone = Rc::clone(&root);
        let cmds_clone = Rc::clone(&commands);
        let effect = create_effect(move || {
            let view = root_clone.borrow().view();
            let flat = reconciler::flatten(view);
            *cmds_clone.borrow_mut() = flat;
        });

        ComponentTree {
            root,
            commands,
            _effect: effect,
        }
    }

    pub fn commands(&self) -> Vec<DrawCommand> {
        self.commands.borrow().clone()
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        batch(|| self.root.borrow_mut().on_event(event))
    }
}

impl Drop for ComponentTree {
    fn drop(&mut self) {
        self.root.borrow().on_unmount();
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
