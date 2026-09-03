use platform_core::WindowConfig;
use renderer_core::Color;
use ui_core::Component;

use crate::app_context::AppCtx;

pub trait App: 'static {
    fn root(&self) -> Box<dyn Component>;

    /// Mounts this app's UI and hands the runner the tree it will drive.
    ///
    /// Mounting belongs to the app because the tree's segment effects must be created in the same reactive
    /// runtime as the signals its `view()` reads. In a normal build there is one runtime and the default is all
    /// there is to it; the dylib-backed `HotApp` overrides this so the tree is mounted *inside* the dylib
    /// instead of on the host's side of the boundary (see [`crate::tree`]).
    #[doc(hidden)]
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        Box::new(crate::tree::LocalTree::new(self.root()))
    }

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
    fn motion_tick(&self, now: web_time::Instant) {
        motion_core::tick(now);
    }

    /// Reports whether any animation is still in flight. See `motion_tick` for why this must be overridden in dev mode.
    #[doc(hidden)]
    fn motion_has_active(&self) -> bool {
        motion_core::has_active()
    }

    /// Reports whether any region redraws itself outside Telar's knowledge. Same dylib-boundary reason as `motion_has_active`.
    #[doc(hidden)]
    fn motion_has_continuous(&self) -> bool {
        motion_core::has_continuous()
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

    /// Drains window-management commands (drag/minimize/maximize/close/set-title) that UI closures enqueued
    /// during event dispatch, so the runner can apply them to the OS window. Only the dylib-backed `HotApp`
    /// overrides this — the command queue is a thread-local that lives in the dylib (where a title bar's
    /// `on_press` pushes to it), so the host must drain it across the FFI boundary rather than its own (empty)
    /// copy, exactly like `dispatch_overlays`.
    #[doc(hidden)]
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        platform_core::take_window_commands()
    }

    /// Reports the OS light/dark preference into the app's theme runtime (drives `follow_system`). Only the
    /// dylib-backed `HotApp` overrides this — the theme signal lives in the dylib's runtime, so the host must
    /// write it across the FFI boundary rather than its own (empty) copy, exactly like `motion_tick`.
    #[doc(hidden)]
    fn set_system_dark(&self, dark: bool) {
        theme_core::set_system_dark(dark);
    }

    /// Runs the completion callbacks of `spawn_task` work that finished since the last frame, writing their
    /// results into signals on the UI thread. Only the dylib-backed `HotApp` overrides this — the pending
    /// callbacks are registered in the dylib's own reactive-core copy (where app code called `spawn_task`),
    /// so the host must drain it across the FFI boundary rather than its own (empty) copy.
    #[doc(hidden)]
    fn drain_tasks(&self) {
        reactive_core::drain_tasks();
    }

    /// Gives the app's reactive runtime the wake a finishing `spawn_task` worker uses to run a frame. Only
    /// the dylib-backed `HotApp` overrides this: without it a task spawned inside the dylib would deliver
    /// its result into a runtime whose waker slot is empty, and nothing would run until the next input event.
    #[doc(hidden)]
    fn install_task_waker(&self, waker: crate::app_context::RedrawWaker) {
        reactive_core::set_task_waker(move || waker.wake());
    }
}

/// Lets [`crate::run_multi_with_platform`] (monomorphic over one `A: App`) drive surfaces of different app types via `Box<dyn App>`.
impl<A: App + ?Sized> App for Box<A> {
    fn root(&self) -> Box<dyn Component> {
        (**self).root()
    }
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        (**self).mount()
    }
    fn clear_color(&self) -> Option<Color> {
        (**self).clear_color()
    }
    fn window_config(&self) -> Option<WindowConfig> {
        (**self).window_config()
    }
    fn on_frame(&mut self, ctx: &mut AppCtx) {
        (**self).on_frame(ctx)
    }
    fn hot_snapshot(&self) -> Option<String> {
        (**self).hot_snapshot()
    }
    fn hot_restore(&self, blob: &str) {
        (**self).hot_restore(blob)
    }
    fn motion_tick(&self, now: web_time::Instant) {
        (**self).motion_tick(now)
    }
    fn motion_has_active(&self) -> bool {
        (**self).motion_has_active()
    }
    fn motion_has_continuous(&self) -> bool {
        (**self).motion_has_continuous()
    }
    fn relayout(&self) {
        (**self).relayout()
    }
    fn begin_event_batch(&self) {
        (**self).begin_event_batch()
    }
    fn end_event_batch(&self) {
        (**self).end_event_batch()
    }
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        (**self).dispatch_overlays(event)
    }
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        (**self).drain_window_commands()
    }
    fn set_system_dark(&self, dark: bool) {
        (**self).set_system_dark(dark)
    }
    fn drain_tasks(&self) {
        (**self).drain_tasks()
    }
    fn install_task_waker(&self, waker: crate::app_context::RedrawWaker) {
        (**self).install_task_waker(waker)
    }
}
