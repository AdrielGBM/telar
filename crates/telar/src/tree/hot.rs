use platform_core::guest::{LentBytes, ReportsSurface};
use platform_core::{AccessNode, ConsumedKeys, Event, TextInput};
use renderer_core::wire::{DrawList, Pixels};
use services_core::guest::LentFrames;
use ui_core::EventResult;
use ui_tree::SegmentNodeInfo;

use super::OwnedTree;

/// A tree that lives on the *app's* side of a hot-reload boundary: the dylib mounts it (so its segment effects are created in the dylib's reactive runtime, subscribed to the dylib's signals) and the host drives it through the entries the `app!` macro exports. Held by the host only as an opaque handle: every method below runs inside the dylib.
///
/// The host's handle holds a share of the loaded dylib, so this instance is always released while its dylib is still mapped, before or after the app it was mounted from.
// `pub` in a crate-private module: `HotEntries`, which the application's crate names through `app!`, carries it in its entry signatures, so it must be public in type while no path outside telar reaches it.
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
    pub fn paint(&mut self) -> LentBytes<DrawList> {
        let commands = self.tree.list.commands();
        self.frames.paint(&commands)
    }

    /// Answers the pixels entry for an image the last [`paint`](Self::paint) drew.
    ///
    /// # Safety
    ///
    /// As [`LentFrames::lend_pixels`].
    pub unsafe fn lend_pixels(&self, image: u64, pixels: *mut LentBytes<Pixels>, found: *mut bool) {
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
#[path = "hot.test.rs"]
mod tests;
