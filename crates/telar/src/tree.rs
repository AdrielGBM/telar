//! The mounted UI as the runner sees it — and the seam that lets the *app's own* runtime own it.
//!
//! An app's widgets, signals, layout nodes and overlays all live in thread-locals. In a normal build there is one
//! copy of each and it does not matter who mounts the tree. Under hot reload the app is a dylib with its own
//! copies: a tree mounted on the host's side registers its segment effects in the *host's* reactive runtime while
//! `view()` reads the *dylib's* signals, so no subscription is ever established and nothing re-renders on its own.
//! The workaround was `bump_force_ticks`, which re-ran every segment and only fired on input events — so an
//! animation, a background thread's result or a timer stayed invisible until the user moved the mouse.
//!
//! So mounting is the app's job: [`App::mount`](crate::app::App::mount) hands the runner a [`UiTree`] it drives
//! through this trait, and a dylib-backed app mounts a [`HotTree`] inside itself. Same shape the plugin subsystem
//! already uses for embedded UIs (`crate::plugin::PluginInstance`). The seam is what removed the workaround:
//! with the tree mounted where its signals live, nothing needs waking by hand.

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

    /// Closes the frame for the input registries this tree's widgets read. A tree in this process shares the
    /// runner's, so the runner's own call covers it; one behind a dylib boundary has a second set of them.
    fn end_frame(&self) {}
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
        // The input registries are read by widgets, and in hot mode those widgets live **here** — so this is
        // where the reading has to be fed. The runner also observes, but on the host side of the boundary,
        // and a `cdylib` carries its own copy of every `thread_local` in `ui-core`: the host was filling one
        // registry while the app read another, empty one. Nothing failed loudly. `modifiers()` answered "no
        // modifiers" and `pointer_buttons()` answered "nothing held", confidently, for the entire life of a
        // `cargo telar dev` session — so a ⇧-click was a plain click and a right-drag was a left-drag, and
        // every gesture built on either silently did the wrong thing while its own tests passed.
        ui_core::observe_keyboard(event);
        ui_core::observe_pointer(event);
        this.tree.on_event(event) == EventResult::Handled
    }

    /// Closes the frame on this side of the boundary, for the same reason [`on_event`](Self::on_event)
    /// observes on it: `key_pressed` answers for one frame, and the frame it answers for is the one whose
    /// widgets asked.
    ///
    /// # Safety
    /// `ptr` must be a live pointer from [`HotTree::mount`].
    pub unsafe fn end_frame(ptr: *mut HotTree) {
        let _ = ptr;
        ui_core::end_keyboard_frame();
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Blank;

    impl crate::app::App for Blank {
        fn root(&self) -> Box<dyn Component> {
            ui_core::reset_layout_runtime();
            Box::new(
                ui_core::Rectangle::new(
                    layout_core::LayoutStyle::new().width(10.0).height(10.0),
                    || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
                )
                .expect("a rectangle builds"),
            )
        }
    }

    /// A tree behind the hot-reload boundary has to feed the input registries **its own** widgets read.
    ///
    /// A `cdylib` carries its own copy of every `thread_local` in `ui-core`, so the runner observing on the
    /// host side filled a registry nothing in the app ever looked at: `modifiers()` answered "none held" and
    /// `pointer_buttons()` answered "nothing pressed", confidently, for a whole `cargo telar dev` session.
    /// A ⇧-click was a plain click and a right-drag was a left-drag, and every gesture built on either did
    /// the wrong thing in the window while passing its own tests — which is exactly the shape of failure a
    /// state registry has, because it never says it does not know.
    ///
    /// The boundary itself cannot be built in a unit test; the property that fixes it can: whoever dispatches
    /// an event observes it.
    #[test]
    fn the_hot_tree_feeds_the_registries_its_own_widgets_read() {
        ui_core::reset_keyboard();
        ui_core::reset_pointer();
        let tree = HotTree::mount(&Blank);

        assert!(!ui_core::modifiers().is_shift);
        unsafe {
            HotTree::on_event(
                tree,
                &Event::ModifiersChanged {
                    modifiers: platform_core::ModifiersState {
                        is_shift: true,
                        ..Default::default()
                    },
                },
            );
        }
        assert!(
            ui_core::modifiers().is_shift,
            "the side that dispatched has to be the side that knows"
        );

        unsafe {
            HotTree::on_event(
                tree,
                &Event::PointerPressed {
                    x: 5.0,
                    y: 5.0,
                    button: platform_core::PointerButton::Secondary,
                    source: platform_core::PointerSource::Mouse,
                },
            );
        }
        assert!(
            ui_core::pointer_buttons().secondary,
            "and the same for which button is down"
        );
        unsafe { HotTree::release(tree) };
        ui_core::reset_keyboard();
        ui_core::reset_pointer();
    }
}
