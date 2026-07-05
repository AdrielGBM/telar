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
}
