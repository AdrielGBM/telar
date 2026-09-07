//! [`AppRuntime`]: the application as the *runner* drives it, which is not the same object an author writes.
//!
//! Everything an application's widgets touch — signals, layout nodes, overlays, the motion registry, the task queue — lives in a thread-local. In an ordinary build there is one copy of each and the runner can reach them directly. Under `cargo telar dev` the application is a dylib with its **own** copies, and every one of those has to be driven across the boundary instead: the host's `motion::tick` advances an empty registry, the host's `drain_tasks` runs callbacks nobody queued, the host's overlay dispatch consults a registry no widget ever registered in.
//!
//! So there are two runtimes, and the runner drives whichever it was handed: [`LocalApp`] for a tree in this process, [`crate::hot::HotApp`] for one behind a dylib. This trait is the difference between them.
//!
//! **It used to be part of [`App`].** Fourteen `#[doc(hidden)]` methods sat on the trait an application implements, every one of them carrying a paragraph explaining that only the dylib-backed app overrides it — so what an author saw was eighteen methods of which four were theirs, and `impl App for Box<A>` redelegated all eighteen by hand. None of it was reachable from application code and none of it belonged in its way.

use platform_core::{AppCtx, Event, RedrawWaker, WindowCommand, WindowConfig};
use renderer_core::Color;
use web_time::Instant;

use crate::app::App;
use crate::tree::{LocalTree, UiTree};

/// The application as the runner drives it: mount its tree, and reach the runtime that tree lives in.
///
/// An application does not implement this — it implements [`App`], and [`LocalApp`] adapts one. Implement it directly only to host a tree the runner cannot reach any other way, which in this repository means exactly one thing: a dylib.
///
/// Every method defaults to the answer for a tree in **this** process, so an implementor overrides only what its own runtime makes untrue.
pub trait AppRuntime: 'static {
    /// Mounts the UI and hands the runner the tree it will drive.
    ///
    /// Mounting belongs here rather than to the runner because a tree's segment effects must be created in the same reactive runtime as the signals its `view()` reads. Mount on the wrong side and no subscription is ever established: the tree renders once and then never again on its own.
    fn mount(&mut self) -> Box<dyn UiTree>;

    fn clear_color(&self) -> Option<Color>;

    fn window_config(&self) -> Option<WindowConfig>;

    /// Called once per frame before rendering.
    fn on_frame(&mut self, ctx: &mut AppCtx);

    /// Serializes the state this application wants to survive a dylib swap, before its library is unloaded.
    fn hot_snapshot(&self) -> Option<String> {
        None
    }

    /// Hands the snapshot the previous library produced to this one, before its tree mounts.
    fn hot_restore(&self, _blob: &str) {}

    /// Advances the motion engine this application's animations were registered in.
    fn motion_tick(&self, now: Instant) {
        motion_core::tick(now);
    }

    /// Whether any animation is still in flight.
    fn motion_has_active(&self) -> bool {
        motion_core::has_active()
    }

    /// Whether any region repaints itself outside Telar's knowledge, which is what keeps the frame generation moving while its draw commands do not change.
    fn motion_has_continuous(&self) -> bool {
        motion_core::has_continuous()
    }

    /// Re-lays out any dirtied layout root, so a reactive change is reflected before the frame is composed.
    fn relayout(&self) {
        ui_core::relayout_if_dirty();
    }

    /// Opens a reactive batch for the duration of event dispatch, paired with [`end_event_batch`](Self::end_event_batch).
    ///
    /// Without one, a signal written by an event handler flushes immediately and re-runs a segment's effect while its widget is still borrowed for `on_event`: that render is skipped and the segment silently loses its reactive subscriptions. Deferring the flush until every borrow is released keeps them.
    fn begin_event_batch(&self) {
        reactive_core::begin_batch();
    }

    fn end_event_batch(&self) {
        reactive_core::end_batch();
    }

    /// Offers a positioned event to the overlay layer before the widget tree sees it, reporting whether an overlay consumed it.
    fn dispatch_overlays(&self, event: &Event) -> bool {
        ui_core::dispatch_overlays(event) == ui_core::EventResult::Handled
    }

    /// Drains the window-management commands a UI closure queued during dispatch — a title bar's drag, a close button — so the runner can apply them to the OS window.
    fn drain_window_commands(&self) -> Vec<WindowCommand> {
        platform_core::take_window_commands()
    }

    /// Reports the OS light/dark preference into the theme runtime that `follow_system` reads.
    fn set_system_dark(&self, dark: bool) {
        theme_core::set_system_dark(dark);
    }

    /// Runs the completion callbacks of `spawn_task` work that finished since the last frame, on the UI thread.
    fn drain_tasks(&self) {
        reactive_core::drain_tasks();
    }

    /// Gives the reactive runtime the wake a finishing worker uses to run a frame. Without it a task delivers its result into a runtime whose waker slot is empty, and nothing runs until the next input event.
    fn install_task_waker(&self, waker: RedrawWaker) {
        reactive_core::set_task_waker(move || waker.wake());
    }
}

/// An [`App`] whose tree lives in this process — every application that is not a hot-reloaded dylib.
///
/// Holds nothing but the app: each of the runtime methods above is already correct for a tree on this side, so this overrides only the four an application actually answers.
pub struct LocalApp<A: App>(pub A);

impl<A: App> AppRuntime for LocalApp<A> {
    fn mount(&mut self) -> Box<dyn UiTree> {
        Box::new(LocalTree::new(self.0.root()))
    }

    fn clear_color(&self) -> Option<Color> {
        self.0.clear_color()
    }

    fn window_config(&self) -> Option<WindowConfig> {
        self.0.window_config()
    }

    fn on_frame(&mut self, ctx: &mut AppCtx) {
        self.0.on_frame(ctx)
    }
}
