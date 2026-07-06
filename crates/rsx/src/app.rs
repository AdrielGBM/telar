use platform_core::WindowConfig;
use renderer_core::Color;
use ui_core::Component;

use crate::app_context::AppCtx;

pub trait App: 'static {
    fn root(&self) -> Box<dyn Component>;

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        None
    }

    /// Called once per frame before rendering. Use `ctx` to request redraws, change renderer backend, or access window signals.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// Hot-reload hook: serialize preserved state before this app's dylib is swapped out. Only the dylib-backed `HotApp` overrides this.
    #[doc(hidden)]
    fn hot_snapshot(&self) -> Option<String> {
        None
    }

    /// Hot-reload hook: hand a snapshot from the previous dylib to this app's dylib before its tree mounts.
    #[doc(hidden)]
    fn hot_restore(&self, _blob: &str) {}

    /// Advances the motion engine. Only the dylib-backed `HotApp` overrides this — in dev mode, host and app dylib link separate copies of motion-core, so the host must tick the app's own registry across the FFI boundary rather than its own (empty) one.
    #[doc(hidden)]
    fn motion_tick(&self, now: std::time::Instant) {
        motion_core::tick(now);
    }

    /// Reports whether any animation is still in flight. See `motion_tick` for why this must be overridden in dev mode.
    #[doc(hidden)]
    fn motion_has_active(&self) -> bool {
        motion_core::has_active()
    }

    /// Re-lays out any dirtied layout root so a reactive change (e.g. a reactive list adding an item)
    /// is reflected before the frame is composed. Only the dylib-backed `HotApp` overrides this — in dev
    /// mode the app's layout tree lives in the dylib's runtime, so the host must relayout across the FFI
    /// boundary rather than its own (empty) copy.
    #[doc(hidden)]
    fn relayout(&self) {
        ui_core::relayout_if_dirty();
    }

    /// Opens a reactive batch in THIS app's runtime for the duration of event dispatch (paired with
    /// `end_event_batch`). Only the dylib-backed `HotApp` overrides it — in dev mode host and app link
    /// separate reactive-core copies with separate runtimes, and the host's own batch cannot reach the
    /// app's. Without batching the app's runtime, a signal written by an event handler flushes immediately,
    /// re-running a segment's effect while its widget is still borrowed for `on_event`; that render is
    /// skipped and the segment silently loses its reactive subscriptions. Deferring the flush until after
    /// dispatch releases every borrow keeps them intact.
    #[doc(hidden)]
    fn begin_event_batch(&self) {
        reactive_core::begin_batch();
    }

    /// Closes the batch opened by `begin_event_batch`, flushing the app's runtime once dispatch is done.
    #[doc(hidden)]
    fn end_event_batch(&self) {
        reactive_core::end_batch();
    }

    /// Routes a positioned pointer event to the overlay layer (modals, dropdowns) with priority over the
    /// tree walk; returns `true` when an overlay consumed it, so the runner blocks the event from the
    /// content behind. Only the dylib-backed `HotApp` overrides it — the overlay registry is a thread-local
    /// that lives in the dylib (where `overlay` widgets register), so the host must consult it across the
    /// FFI boundary rather than its own (empty) copy.
    #[doc(hidden)]
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        ui_core::dispatch_overlays(event) == ui_core::EventResult::Handled
    }
}
