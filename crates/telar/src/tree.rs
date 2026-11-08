//! The mounted UI as the runner sees it, and the seam that lets the *app's own* runtime own it.
//!
//! An app's widgets, signals, layout nodes and overlays all live in thread-locals. In a normal build there is one copy of each and it does not matter who mounts the tree. Under hot reload the app is a dylib with its own copies: a tree mounted on the host's side would register its segment effects in the *host's* reactive runtime while `view()` reads the *dylib's* signals, so no subscription would ever be established and nothing would re-render on its own.
//!
//! So mounting belongs to the runtime the tree lives in: [`AppRuntime::mount`](crate::AppRuntime::mount) hands the runner a [`UiTree`] it drives through this trait, and a dylib-backed app mounts a `HotTree` inside itself, the shape `telar-embed` uses for an embedded UI, for the same reason.

use std::cell::Ref;
use std::ops::Deref;

use platform_core::Event;
use renderer_core::DrawCommand;
use ui_core::{Component, ComponentList, EventResult, OwnedBuild};
use ui_tree::SegmentNodeInfo;

#[cfg(all(
    feature = "dev",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub(crate) mod hot;

/// One composed frame: borrowed straight from the segment cache when the tree is in this process, owned when it was decoded from what a dylib lent.
pub enum Frame<'a> {
    Borrowed(Ref<'a, Vec<DrawCommand>>),
    Owned(Vec<DrawCommand>),
}

impl Deref for Frame<'_> {
    type Target = [DrawCommand];

    fn deref(&self) -> &[DrawCommand] {
        match self {
            Frame::Borrowed(r) => r,
            Frame::Owned(v) => v,
        }
    }
}

/// The mounted UI the runner drives: event dispatch, composition, and the dirtiness/generation gates it uses to decide whether a frame is worth rendering.
pub trait UiTree {
    fn on_event(&mut self, event: &Event) -> EventResult;

    /// Hands the tree a move the overlay layer took, with everything in it covered; see [`ui_core::covered`].
    fn on_covered_event(&mut self, event: &Event);

    /// The composed draw commands for this frame.
    fn frame(&self) -> Frame<'_>;

    /// Whether the composition changed since the last frame — the runner's gate for skipping work.
    fn is_dirty(&self) -> bool;

    /// Content generation: two equal reads mean [`frame`](Self::frame) would return identical commands.
    fn generation(&self) -> u64;

    /// Pre-order walk for the devtools inspector.
    fn walk(&self, out: &mut Vec<SegmentNodeInfo>);

    /// Closes the frame for the input registries this tree's widgets read. A tree in this process shares the runner's, so the runner's own call covers it; one behind a dylib boundary has a second set of them.
    fn end_frame(&self) {}
}

/// A [`ComponentList`] that owns everything its root's build made, so dropping it frees the whole tree: the effects and signals the build created, the widgets those effects keep alive, focus registrations with them, and the layout nodes the tree is laid out on. A [`ComponentList`] alone frees nothing it did not create: a mounted tree lives as long as the owner it was built under. Without an owner of its own, a remount left the previous tree running in the surface's root owner until the surface closed. With only the owner, it left the previous tree's nodes behind with their rect signals freed, its root still the overlay host the next tree's overlays attached to.
struct OwnedTree {
    list: ComponentList,
    _build: OwnedBuild,
}

impl OwnedTree {
    fn mount(build: impl FnOnce() -> Box<dyn Component>) -> Self {
        let (list, built) = OwnedBuild::run(|| ComponentList::new(build()));
        Self {
            list,
            _build: built,
        }
    }
}

/// A tree mounted in this process, on top of a [`ComponentList`].
pub struct LocalTree(OwnedTree);

impl LocalTree {
    /// Builds the root with `build` and mounts it, owning what the build made: dropping the tree frees all of it.
    pub fn new(build: impl FnOnce() -> Box<dyn Component>) -> Self {
        Self(OwnedTree::mount(build))
    }
}

impl UiTree for LocalTree {
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.list.on_event(event)
    }

    fn on_covered_event(&mut self, event: &Event) {
        ui_core::covered(|| self.0.list.on_event(event));
    }

    fn frame(&self) -> Frame<'_> {
        Frame::Borrowed(self.0.list.commands())
    }

    fn is_dirty(&self) -> bool {
        self.0.list.is_dirty()
    }

    fn generation(&self) -> u64 {
        self.0.list.generation()
    }

    fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.0.list.walk_tree(out);
    }
}

#[cfg(test)]
#[path = "tree.test.rs"]
mod tests;
