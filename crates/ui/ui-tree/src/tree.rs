//! [`ComponentList`]: the mounted tree, and the entry point a runner dispatches events and asks frames of.

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::batch;
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::segment::{self, Segment, SegmentRoot};

/// The mounted tree: what a runner dispatches events into and asks each frame's commands of.
///
/// Dropping it frees nothing: the tree, its root's render effect included, lives as long as the owner it was built and mounted under, whose effects keep every widget alive. Build and mount under an owner of the tree's own and dispose that, as the runner does, to free a tree and the focus registrations of its controls.
pub struct ComponentList {
    // Shared with the root segment, which borrows it immutably to render while `on_event` borrows it mutably.
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

    /// This tree drawn inside another one. See [`SegmentRoot::boundary`].
    pub fn boundary(&self) -> crate::RenderNode {
        self.segment_root.boundary()
    }

    /// Emits the component tree in pre-order for the devtools inspector. See [`SegmentRoot::walk`].
    pub fn walk_tree(&self, out: &mut Vec<segment::SegmentNodeInfo>) {
        self.segment_root.walk(out);
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        self.dispatch(event)
    }

    /// [`on_event`](Self::on_event) through a shared reference, for a holder that hands the tree out while it routes events into it: the root is borrowed only for the dispatch itself.
    pub fn dispatch(&self, event: &Event) -> EventResult {
        // Signals mutated by handlers flush their effects after on_event releases the borrow; overlay routing happens in the runner via App::dispatch_overlays.
        batch(|| self.root.borrow_mut().on_event(event))
    }
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tests;
