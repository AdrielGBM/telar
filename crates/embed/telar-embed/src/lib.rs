//! Embedded apps: hosting a separately-`dlopen`'d rsx UI inside a host window.
//!
//! Unlike hot-reload (dev-only, one dylib that *replaces* the whole window), an embedded app is a production capability: the host stays a full rsx app and embeds one or more guests, each a cdylib with its **own** reactive/layout/overlay/motion runtime (separate thread-locals, because each dylib statically links its own copy of the runtime crates). The host cannot reach into that runtime, so — exactly as hot-reload does for its single dylib — it *drives* the guest through the entries the guest exports.
//!
//! The novel part hot-reload never needed is **compositing two runtimes into one window**: the guest flattens its own view tree to a self-contained draw list and lends it as bytes, which the host decodes into a [`DrawList`] of its own ([`LoadedEmbed::paint`]), translates and clips into the guest's sub-rect ([`composite`]) and splices into its own frame. No offscreen texture, no shared GPU device — the host's renderer paints everything in one pass.
//!
//! Host and guest meet on the guest contract both boundaries share ([`platform_core::guest`]): one `repr(C)` table of `extern "C"` entries ([`EmbedEntries`]), stamped with [`EMBED_CONTRACT`], through which nothing owned crosses. Each side frees only what it allocated, so a `#[global_allocator]` on either side does not matter, and a guest built against another contract, with other features, or as a hot-reload library is refused before any of its code runs but its initializers.
//!
//! Layering: this crate is app-agnostic. A guest author implements [`EmbeddedApp`] (or an adapter to it) and calls the [`embed!`](crate::embed) macro to export the entries. The host enables `host`, calls [`load_embedded`] and drives the returned [`LoadedEmbed`]. Nothing here knows about any particular app.
//!
//! **A second dependency, like `telar-dynamic`.** It knows nothing of `telar` — a guest builds a `ui-core` tree and hands back draw commands, which is a layer below the facade — so it is the application that names both, and keeping them on one version is the same lockstep `telar` and `telar-macros` already have.
//!
//! # Feature flags
#![cfg_attr(
    feature = "document-features",
    doc = document_features::document_features!()
)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod entries;
#[doc(hidden)]
pub mod guest;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
mod host;

use std::cell::{Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;

use geometry_core::{Rect, Size, Transform};
use platform_core::{AppCtx, Event, SystemPreferences, WindowCommand};
use renderer_core::{Color, DrawCommand};
use ui_core::{EventResult, NodeId, SurfaceCanvas, composite_surface};
use ui_tree::{Component, RenderNode};

#[doc(hidden)]
pub use entries::{CreateInstance, EMBED_CONTRACT, EmbedEntries};
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub use host::{LoadedEmbed, load_embedded};
#[doc(hidden)]
pub use platform_core::export_contract_stamp;
pub use platform_core::guest::GuestPanic;
#[doc(hidden)]
pub use platform_core::guest::{CallOutcome, Handle, LentBytes};

/// A guest's frame as the host holds it: decoded from the bytes the guest lent into commands the host allocated and owns.
pub type DrawList = Vec<DrawCommand>;
/// The window-management commands drained from a guest's own queue (a title bar's drag, close and so on), decoded into the host's own.
pub type WindowCommands = Vec<WindowCommand>;

/// Wrap a guest's painted [`DrawList`] into a [`RenderNode`] the host splices into its own frame: translated to `rect`'s origin (the guest paints in its own `(0,0)` space) and clipped to `rect` (so it can't draw over the host chrome). The exact idiom rsx's own scroll area uses to host a sub-tree in a viewport.
pub fn composite(rect: Rect, commands: DrawList) -> RenderNode {
    composite_surface(
        Transform::translate(rect.x, rect.y),
        Size::new(rect.width, rect.height),
        commands.into_iter().map(RenderNode::Primitive),
    )
}

/// An embeddable rsx UI a host can drive as a guest. The generic union of "build a view tree, render it, handle events, run per-frame background work, and present a title/icon" — no app-specific semantics. A concrete app (or an adapter over one) implements this; the [`embed!`](crate::embed) macro exports it.
///
/// Lifecycle the driver enforces: [`build`](Self::build) runs once, inside the guest's freshly-entered [`Surface`](ui_core::Surface), so the content's layout nodes land in *this* surface's world; afterwards [`layout_root`](Self::layout_root) is the node the driver sizes to the host's sub-rect.
pub trait EmbeddedApp: 'static {
    /// Build the content's layout tree. Called once by the driver with the guest's surface active, so nodes are allocated in this surface's layout world. [`layout_root`](Self::layout_root) must be valid after it.
    ///
    /// **Also where a guest installs what only a runner would have installed for it.** A guest is a `cdylib`: it links its own copy of every crate behind the facade, so the statics and thread-locals those keep are its own and start empty. The host's process-wide backends are installed before this runs, from the [`HostServices`](platform_core::guest::HostServices) table it serves: the clipboard, the URI opener, file dialogs, the preference store, the app's paths and name, the keyboard convention, the location format, the asset base, the layout grid, the loop wake and, for a guest with `shaper`, the font database its shaper measures in. The theme is not among them; `TextMetrics` is the one that bites, because nothing sets a default and the first text laid out here panics without it — call `telar::install_default_text_metrics()` (or `renderer_core::set_text_metrics` with whatever suits the surface) before building the tree. The driver cannot do it: a measurer is code, not a backend the table serves, and only the guest knows whether it draws to pixels or to cells.
    fn build(&mut self);

    /// The content's top layout node — the one the driver `compute_layout`s to the host-assigned rect size.
    fn layout_root(&self) -> NodeId;

    /// Render the content to a [`RenderNode`] (flattened by the driver into the returned [`DrawList`]).
    fn view(&self) -> RenderNode;

    /// Route an event (already translated into the content's local coordinate space by the host).
    fn on_event(&mut self, event: &Event) -> EventResult;

    /// Runs after the driver has laid out the root at the new size, for internal scroll viewports to react to it. No-op for content without scroll areas.
    fn relayout_viewports(&mut self) {}

    /// Called when the content becomes visible (the host activated its tab). Autofocus the primary input here.
    fn activate(&mut self) {}

    /// Drain background-work channels into signals (see [`platform_core::RedrawWaker`]); the host forwards its `ctx`.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// The window/tab clear color, if the content wants one.
    fn clear_color(&self) -> Option<Color> {
        None
    }

    /// The guest's display title (may read signals — the driver reads it with the surface active).
    fn title(&self) -> String;

    /// The guest's icon bytes, which cross to the host spelled as bytes it copies into its own.
    fn icon(&self) -> Option<Vec<u8>> {
        None
    }

    /// A stable identifier for this app kind (routing, discovery).
    fn id(&self) -> String;
}

/// Bridges the driver's [`ComponentList`] (which owns its root [`Component`]) to the shared [`EmbeddedApp`], so the driver keeps its own handle to call `activate`/`relayout_viewports`/metadata while the segment tree renders and dispatches events through the same object. Single-threaded; the borrows never overlap (paint borrows during `commands()`, events during `on_event`, driver calls in between).
struct EmbeddedComponent(Rc<RefCell<Box<dyn EmbeddedApp>>>);

impl Component for EmbeddedComponent {
    fn view(&self) -> RenderNode {
        self.0.borrow().view()
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.borrow_mut().on_event(event)
    }
    fn debug_name(&self) -> &'static str {
        "EmbedRoot"
    }
}

/// The dylib-side guest driver: a headless single-surface runtime (no window, no renderer) that the host drives through the entries its library exports. A [`SurfaceCanvas`] over the guest's content, so every method works in this guest's thread-local worlds, not the host's.
///
/// The host holds it only behind an opaque [`Handle`], with the frames it lends ([`guest::GuestInstance`]), and never reads through it: every call goes through an entry, so the code runs in the dylib.
pub struct EmbedInstance {
    embedded: Rc<RefCell<Box<dyn EmbeddedApp>>>,
    canvas: SurfaceCanvas,
}

impl EmbedInstance {
    /// Build the guest: allocate its surface, build the content tree inside it, and mount the segment tree.
    pub fn new(embedded: Box<dyn EmbeddedApp>) -> Self {
        let embedded = Rc::new(RefCell::new(embedded));
        let canvas = SurfaceCanvas::with_root(Size::new(0.0, 0.0), || {
            embedded.borrow_mut().build();
            let root = embedded.borrow().layout_root();
            Ok((EmbeddedComponent(Rc::clone(&embedded)), root))
        })
        .expect("an embedded app's build reports no layout error");
        Self { embedded, canvas }
    }

    /// Lay the content out to the host-assigned sub-rect size, which is this guest's surface size, then let its scroll viewports react to the new size.
    pub fn relayout(&mut self, width: f32, height: f32) {
        self.canvas.lay_out(Size::new(width, height));
        self.canvas.with_entered(|| {
            // So a signal the content writes flushes after the `borrow_mut` is released; a synchronous flush would re-run the segment's `view()`, which borrows the same `RefCell`.
            let embedded = &self.embedded;
            reactive_core::batch(|| embedded.borrow_mut().relayout_viewports());
        })
    }

    /// Re-lay-out only what the guest's own reactive changes dirtied (a list grew, a panel toggled), at the last size given to [`relayout`](Self::relayout). Driven every frame by the host — the analog of the runner calling `App::relayout` (`ui_core::relayout_if_dirty`) on an in-process app.
    pub fn relayout_dirty(&self) {
        self.canvas.relayout_if_dirty();
    }

    /// The guest's current frame as a flat, self-contained command list, which the guest's entry encodes for the host to decode, translate into the guest's sub-rect and splice into its own frame.
    pub fn paint(&self) -> Ref<'_, Vec<DrawCommand>> {
        self.canvas.frame_commands()
    }

    /// The content generation; unchanged between two reads means [`paint`](Self::paint) would return the same commands, so the host can skip re-fetching (mirrors the host renderer's idle-blit gate).
    pub fn generation(&self) -> u64 {
        self.canvas.generation()
    }

    /// Whether an animation is still in flight in this guest's motion engine.
    pub fn motion_has_active(&self) -> bool {
        self.canvas.with_entered(motion_core::has_active)
    }

    /// Dispatch an event to the content (already in local coordinates). Self-batches in the guest's runtime.
    pub fn on_event(&mut self, event: &Event) -> bool {
        self.canvas.with_entered(|| {
            // A cdylib carries its own copy of every `thread_local` in ui-core, so observing on the host's side left the guest's widgets reading a permanently empty registry. `HotTree::on_event` carries these for the same reason.
            ui_core::observe_keyboard(event);
            ui_core::observe_pointer(event);
            self.canvas.dispatch_tree(event) == EventResult::Handled
        })
    }

    /// Route a positioned event to the guest's overlay layer (modals/dropdowns) with priority; `true` means an overlay consumed it and the host should not fall through to the content.
    ///
    /// The host calls this before [`on_event`](Self::on_event) and stops when it returns `true`, so the registries are fed here too — otherwise an event an overlay consumes never reaches them at all. For the same reason a move an overlay takes is handed to the content here, [`covered`](ui_core::covered), so a box hovered under the overlay hears the pointer leave it.
    pub fn dispatch_overlays(&self, event: &Event) -> bool {
        self.canvas.with_entered(|| {
            ui_core::observe_keyboard(event);
            ui_core::observe_pointer(event);
            let taken = self.canvas.dispatch_overlays(event) == EventResult::Handled;
            if taken && matches!(event, Event::PointerMoved { .. }) {
                ui_core::covered(|| self.canvas.dispatch_tree(event));
            }
            taken
        })
    }

    /// Closes the frame on this side of the boundary, for the same reason [`on_event`](Self::on_event) observes on it: `key_pressed` answers for one frame, and the frame it answers for is the one whose widgets asked.
    pub fn end_frame(&self) {
        self.canvas.with_entered(|| {
            ui_core::end_keyboard_frame();
        })
    }

    /// Advance the guest's motion engine and flush its runtime so animations progress and re-render.
    pub fn motion_tick(&self, now: Instant) {
        self.canvas
            .with_entered(|| reactive_core::batch(|| motion_core::tick(now)))
    }

    /// Drain window-management commands the guest's UI enqueued (its title bar drag/minimize/close).
    pub fn drain_window_commands(&self) -> WindowCommands {
        self.canvas.take_window_commands()
    }

    /// Write the user's system preferences into the guest's own copy of the store, which its views, its theme's `follow_system` and its motion engine read.
    pub fn set_system_preferences(&self, preferences: &SystemPreferences) {
        self.canvas.with_entered(|| {
            preferences_core::set_system_preferences(preferences.clone());
        })
    }

    /// Run the guest's due timers, its finished tasks and then its per-frame background-work hook, forwarding the host's `ctx` (so a guest worker thread can wake the host loop via `ctx.redraw_waker()`, just as an in-process app does).
    pub fn on_frame(&mut self, ctx: &mut AppCtx) {
        self.canvas.with_entered(|| {
            // The guest links its own reactive-core copy, so `spawn_task` and `run_after` inside it register in a runtime the host cannot reach: they are run here, in the runner's order.
            reactive_core::fire_timers();
            reactive_core::drain_tasks();
            // So signals the hook writes flush after the `borrow_mut` releases, never re-entering `view()` mid-borrow.
            let embedded = &self.embedded;
            reactive_core::batch(|| embedded.borrow_mut().on_frame(ctx));
        })
    }

    /// How long until the guest's earliest [`run_after`](reactive_core::run_after) timer comes due, which [`on_frame`](Self::on_frame) fires; `None` when none is waiting.
    pub fn until_next_timer(&self) -> Option<Duration> {
        reactive_core::until_next_timer()
    }

    /// When that timer comes due, on the guest's own timer clock: the same answer until the deadline itself moves. See [`reactive_core::next_timer_due`].
    pub fn next_timer_due(&self) -> Option<Instant> {
        reactive_core::next_timer_due()
    }

    /// Autofocus/announce the content becoming visible; re-render so a focus change shows this frame.
    pub fn activate(&mut self) {
        self.canvas.with_entered(|| {
            let embedded = &self.embedded;
            reactive_core::batch(|| embedded.borrow_mut().activate());
        })
    }

    pub fn clear_color(&self) -> Option<Color> {
        self.canvas
            .with_entered(|| self.embedded.borrow().clear_color())
    }

    pub fn title(&self) -> String {
        self.canvas.with_entered(|| self.embedded.borrow().title())
    }

    pub fn icon(&self) -> Option<Vec<u8>> {
        self.canvas.with_entered(|| self.embedded.borrow().icon())
    }

    pub fn id(&self) -> String {
        self.canvas.with_entered(|| self.embedded.borrow().id())
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
