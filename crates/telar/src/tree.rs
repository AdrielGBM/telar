//! The mounted UI as the runner sees it — and the seam that lets the *app's own* runtime own it.
//!
//! An app's widgets, signals, layout nodes and overlays all live in thread-locals. In a normal build there is one
//! copy of each and it does not matter who mounts the tree. Under hot reload the app is a dylib with its own
//! copies: a tree mounted on the host's side registers its segment effects in the *host's* reactive runtime while
//! `view()` reads the *dylib's* signals, so no subscription is ever established and nothing re-renders on its own
//! (the historical `bump_force_ticks` workaround, which only fires on input events — so an animation, a
//! background thread's result or a timer stays invisible until the user moves the mouse).
//!
//! So mounting is the app's job: [`App::mount`](crate::app::App::mount) hands the runner a [`UiTree`] it drives
//! through this trait, and a dylib-backed app mounts a [`HotTree`] inside itself. Same shape the plugin subsystem
//! already uses for embedded UIs (`crate::plugin::PluginInstance`).

use std::cell::Ref;
use std::ops::Deref;

use platform_core::Event;
use renderer_core::DrawCommand;
use ui_core::{Component, ComponentList, EventResult};
use ui_tree::SegmentNodeInfo;

/// One composed frame: borrowed straight from the segment cache when the tree is in this process, owned when it
/// had to be copied out of a dylib.
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

/// The mounted UI the runner drives: event dispatch, composition, and the dirtiness/generation gates it uses to
/// decide whether a frame is worth rendering.
pub trait UiTree {
    fn on_event(&mut self, event: &Event) -> EventResult;

    /// The composed draw commands for this frame.
    fn frame(&self) -> Frame<'_>;

    /// Whether the composition changed since the last frame — the runner's gate for skipping work.
    fn is_dirty(&self) -> bool;

    /// Content generation: two equal reads mean [`frame`](Self::frame) would return identical commands.
    fn generation(&self) -> u64;

    /// Pre-order walk for the devtools inspector.
    fn walk(&self, out: &mut Vec<SegmentNodeInfo>);

    /// Re-run every segment's view effect. Only a tree whose signals live in another runtime than its effects
    /// needs this; one that owns both re-renders from its own subscriptions and leaves it a no-op.
    fn bump_force_ticks(&self) {}
}

/// A tree mounted in this process, on top of a [`ComponentList`].
pub struct LocalTree(ComponentList);

impl LocalTree {
    pub fn new(root: Box<dyn Component>) -> Self {
        Self(ComponentList::new(root))
    }
}

impl UiTree for LocalTree {
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.on_event(event)
    }

    fn frame(&self) -> Frame<'_> {
        Frame::Borrowed(self.0.commands())
    }

    fn is_dirty(&self) -> bool {
        self.0.is_dirty()
    }

    fn generation(&self) -> u64 {
        self.0.generation()
    }

    fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.0.walk_tree(out);
    }

    fn bump_force_ticks(&self) {
        self.0.bump_force_ticks();
    }
}

/// A tree that lives on the *app's* side of a hot-reload boundary: the dylib mounts it (so its segment effects
/// are created in the dylib's reactive runtime, subscribed to the dylib's signals) and the host drives it through
/// the shims the `app!` macro exports. Held by the host only as an opaque pointer — every method below runs
/// inside the dylib.
///
/// The runner must drop its tree *before* replacing the app, so this instance is freed while its dylib is still
/// mapped; `AppHandler`'s reload path already does that for the same reason effect closures require it.
pub struct HotTree {
    tree: ComponentList,
}

impl HotTree {
    /// Mounts the app's root inside this dylib and hands the host an opaque pointer to it.
    ///
    /// # Safety
    /// The returned pointer must be freed with [`HotTree::release`], from this same dylib, before it is unloaded.
    pub fn mount(app: &dyn crate::app::App) -> *mut HotTree {
        Box::into_raw(Box::new(HotTree {
            tree: ComponentList::new(app.root()),
        }))
    }

    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`], not yet released.
    pub unsafe fn release(ptr: *mut HotTree) {
        drop(unsafe { Box::from_raw(ptr) });
    }

    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn on_event(ptr: *mut HotTree, event: &Event) -> bool {
        let this = unsafe { &mut *ptr };
        this.tree.on_event(event) == EventResult::Handled
    }

    /// Copies this frame's commands out to the host: the segment cache cannot be lent across the boundary,
    /// because the borrow would outlive the call the dylib controls.
    ///
    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn paint(ptr: *mut HotTree) -> Vec<DrawCommand> {
        let this = unsafe { &*ptr };
        this.tree.commands().clone()
    }

    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn is_dirty(ptr: *mut HotTree) -> bool {
        let this = unsafe { &*ptr };
        this.tree.is_dirty()
    }

    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn generation(ptr: *mut HotTree) -> u64 {
        let this = unsafe { &*ptr };
        this.tree.generation()
    }

    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn walk(ptr: *mut HotTree) -> Vec<SegmentNodeInfo> {
        let this = unsafe { &*ptr };
        let mut out = Vec::new();
        this.tree.walk_tree(&mut out);
        out
    }
}

/// Adapts a [`UiTree`] to the devtools inspector's view of a tree.
pub struct TreeView<'a>(pub &'a dyn UiTree);

impl devtools_core::DevTreeView for TreeView<'_> {
    fn node_count(&self) -> usize {
        let mut nodes = Vec::new();
        self.0.walk(&mut nodes);
        nodes.len()
    }

    fn for_each_node(&self, f: &mut dyn FnMut(&devtools_core::DevNodeInfo)) {
        let mut nodes = Vec::new();
        self.0.walk(&mut nodes);
        for node in &nodes {
            f(&devtools_core::DevNodeInfo {
                id: node.id,
                name: node.name,
                rect: node.rect,
                depth: node.depth,
            });
        }
    }
}
