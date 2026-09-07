//! [`ComponentList`]: the mounted tree, and the entry point a runner dispatches events and asks frames of.

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use platform_core::Event;
use reactive_core::batch;
use renderer_core::DrawCommand;

use crate::component::{Component, EventResult};
use crate::segment::{self, Segment, SegmentRoot};

/// The mounted tree: what a runner dispatches events into and asks each frame's commands of.
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

    /// Emits the component tree in pre-order for the devtools inspector. See [`SegmentRoot::walk`].
    pub fn walk_tree(&self, out: &mut Vec<segment::SegmentNodeInfo>) {
        self.segment_root.walk(out);
    }

    pub fn on_event(&mut self, event: &Event) -> EventResult {
        // So signals mutated by handlers flush their effects after `on_event` returns and releases the borrow. Overlay priority routing is not done here: it must run on the side that owns the overlay registry, which under hot reload is the app dylib rather than the host holding this `ComponentList`. The runner consults it via `App::dispatch_overlays` before calling this, and skips this call when an overlay consumed it.
        batch(|| self.root.borrow_mut().on_event(event))
    }
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tests;
