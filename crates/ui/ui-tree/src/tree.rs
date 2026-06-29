use std::cell::{Ref, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::batch;
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::segment::{self, Segment, SegmentRoot};

pub struct ComponentList {
    // Shared with the root segment: the segment borrows it immutably to render; on_event borrows it mutably. They never overlap because event dispatch is batched (flush happens after on_event).
    root: Rc<RefCell<dyn Component>>,
    segment_root: SegmentRoot,
}

impl ComponentList {
    pub fn new<C: Component + 'static>(component: C) -> Self {
        let root: Rc<RefCell<dyn Component>> = Rc::new(RefCell::new(component));
        let seg = Segment::mount_dyn(Rc::clone(&root));
        Self {
            root,
            segment_root: SegmentRoot::from_segment(seg),
        }
    }

    /// Current content generation. Increments whenever the composed draw commands are rebuilt. Two reads returning the same value guarantee identical `commands()` output.
    pub fn generation(&self) -> u64 {
        self.segment_root.generation()
    }

    pub fn is_dirty(&self) -> bool {
        self.segment_root.is_dirty()
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        self.segment_root.commands()
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        // Batch so any signals mutated by handlers flush their effects AFTER on_event returns (and releases the borrow_mut), never re-entering a segment effect mid-borrow.
        batch(|| self.root.borrow_mut().on_event(event))
    }

    // In hot-reload mode the dylib's reactive signals are not tracked by the binary's effects, so state changes from on_event (e.g. WindowResized updating layout) would never trigger a re-render. Call this after on_event to force every segment's view effect to re-run so it reads fresh layout and state.
    pub fn bump_force_ticks(&self) {
        batch(segment::bump_force_ticks);
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use reactive_core::signal;
    use renderer_core::{Color, RectStyle, ShapeStyle};
    use std::sync::Arc;

    use super::*;
    use crate::render_node::RenderNode;

    fn sample_rect(x: f32) -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            style: Arc::new(RectStyle::default().with_fill(Color::BLACK)),
        }
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
        let tree = ComponentList::new(Fixed);
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
        let signal = signal(2i32);
        let tree = ComponentList::new(Counter {
            value: signal.clone(),
        });

        assert_eq!(tree.commands().len(), 2);

        signal.set(5);
        assert_eq!(tree.commands().len(), 5);

        signal.set(0);
        assert_eq!(tree.commands().len(), 0);
    }
}
