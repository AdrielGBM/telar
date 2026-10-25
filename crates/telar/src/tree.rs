//! The mounted UI as the runner sees it, and the seam that lets the *app's own* runtime own it.
//!
//! An app's widgets, signals, layout nodes and overlays all live in thread-locals. In a normal build there is one copy of each and it does not matter who mounts the tree. Under hot reload the app is a dylib with its own copies: a tree mounted on the host's side would register its segment effects in the *host's* reactive runtime while `view()` reads the *dylib's* signals, so no subscription would ever be established and nothing would re-render on its own.
//!
//! So mounting belongs to the runtime the tree lives in: [`AppRuntime::mount`](crate::AppRuntime::mount) hands the runner a [`UiTree`] it drives through this trait, and a dylib-backed app mounts a [`HotTree`] inside itself, the shape `telar-embed` uses for an embedded UI, for the same reason.

use std::cell::Ref;
use std::ops::Deref;

use platform_core::guest::LentBytes;
use platform_core::guest::ReportsSurface;
use platform_core::{AccessNode, ConsumedKeys, Event, TextInput};
use renderer_core::DrawCommand;
use services_core::guest::LentFrames;
use ui_core::{Component, ComponentList, EventResult, OwnedBuild};
use ui_tree::SegmentNodeInfo;

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

/// A tree that lives on the *app's* side of a hot-reload boundary: the dylib mounts it (so its segment effects are created in the dylib's reactive runtime, subscribed to the dylib's signals) and the host drives it through the entries the `app!` macro exports. Held by the host only as an opaque handle: every method below runs inside the dylib.
///
/// The host's handle holds a share of the loaded dylib, so this instance is always released while its dylib is still mapped, before or after the app it was mounted from.
pub struct HotTree {
    tree: OwnedTree,
    frames: LentFrames,
}

impl HotTree {
    /// Mounts the app's root inside this dylib.
    pub fn mount(app: &dyn crate::app::App) -> Self {
        Self {
            tree: OwnedTree::mount(|| app.root()),
            frames: LentFrames::new(),
        }
    }

    pub fn on_event(&mut self, event: &Event) -> bool {
        // A dylib carries its own copy of the input registries its widgets read, so the host observing on its side would leave these reading an empty one: modifiers and held buttons are fed here.
        ui_core::observe_keyboard(event);
        ui_core::observe_pointer(event);
        let followed = ui_core::follow_box_event(event, |event| {
            reactive_core::batch(|| {
                if ui_core::dispatch_overlays(event) != EventResult::Handled {
                    self.on_event(event);
                }
            })
        });
        followed.unwrap_or_else(|| self.tree.list.on_event(event) == EventResult::Handled)
    }

    /// [`on_event`](Self::on_event) for a move the overlay layer took, with everything in the tree covered; see [`ui_core::covered`].
    pub fn on_covered_event(&mut self, event: &Event) {
        ui_core::covered(|| self.tree.list.on_event(event));
    }

    /// Closes the frame on this side of the boundary, for the same reason [`on_event`](Self::on_event) observes on it: `key_pressed` answers for one frame, and the frame it answers for is the one whose widgets asked.
    pub fn end_frame(&self) {
        ui_core::end_keyboard_frame();
    }

    /// This frame's commands, lent as [`LentFrames::paint`] lends them.
    pub fn paint(&mut self) -> LentBytes {
        let commands = self.tree.list.commands();
        self.frames.paint(&commands)
    }

    /// Answers the pixels entry for an image the last [`paint`](Self::paint) drew.
    ///
    /// # Safety
    ///
    /// As [`LentFrames::lend_pixels`].
    pub unsafe fn lend_pixels(&self, image: u64, pixels: *mut LentBytes, found: *mut bool) {
        unsafe { self.frames.lend_pixels(image, pixels, found) }
    }

    /// Has the next [`paint`](Self::paint) stand alone, for a host that could not read one of the frames before it.
    pub fn reset_frames(&mut self) {
        self.frames.reset();
    }

    pub fn is_dirty(&self) -> bool {
        self.tree.list.is_dirty()
    }

    pub fn generation(&self) -> u64 {
        self.tree.list.generation()
    }

    /// The tree, pre-order, as the devtools inspector reads it.
    pub fn walk(&self) -> Vec<SegmentNodeInfo> {
        let mut nodes = Vec::new();
        self.tree.list.walk_tree(&mut nodes);
        nodes
    }

    /// What a reader is told about the frame this tree composed, and what the accessibility checks find in it, for the inspector; see [`crate::inspect::audit`].
    pub fn access_audit(
        &self,
        background: Option<renderer_core::Color>,
    ) -> platform_core::AccessAudit {
        crate::inspect::audit(&self.tree.list.commands(), background)
    }
}

/// What the tree tells the host about the window it fills, read in the registries of the library it was mounted in, which the host's copies of them never see.
impl ReportsSurface for HotTree {
    fn access_snapshot(&self) -> Vec<AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.list.commands())
    }

    fn perform_access_action(&mut self, id: u64, activate: bool) {
        self.on_event(&Event::AccessAction { id, activate });
    }

    fn text_input(&self) -> Option<TextInput> {
        ui_core::text_input()
    }

    fn arrive(&mut self, backwards: bool) -> bool {
        ui_core::focus::arrive(backwards)
    }

    fn tab(&mut self, key: &Event) -> bool {
        ui_core::focus::tab(key, |key| {
            if self.on_event(key) {
                EventResult::Handled
            } else {
                EventResult::Ignored
            }
        })
    }

    fn blur(&mut self) {
        ui_core::focus::clear();
    }

    fn holds_focus(&self) -> bool {
        ui_core::focus::current().is_some()
    }

    fn text_entry_focused(&self) -> bool {
        ui_core::focus::text_entry_focused()
    }

    fn keeps(&self, keys: ConsumedKeys) -> bool {
        ui_core::focus::focused_keeps(keys)
    }
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tests;
