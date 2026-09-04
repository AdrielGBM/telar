//! [`AppCtx`]: what an application is handed each frame — its window handles, its redraw waker and its paths.

use std::sync::Arc;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// A `Send`/`Sync`, cloneable handle that wakes the UI event loop (requests a redraw). Hand it to a background thread (a file dialog, a `notify` watcher, a network fetch) so that when the thread has produced a result — delivered over a channel — it can wake the loop; the app then drains the channel in [`crate::App::on_frame`] and writes the result into signals. This is the bridge between off-thread work and the single-threaded reactive/UI world (signals are `!Send`, so the data itself must cross via a channel, not a signal).
#[derive(Clone)]
pub struct RedrawWaker(Arc<dyn Fn() + Send + Sync>);

impl RedrawWaker {
    pub(crate) fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// Wakes the UI loop so the next frame runs (and `on_frame` polls for results). Cheap and idempotent.
    pub fn wake(&self) {
        (self.0)()
    }
}

/// What an application is handed each frame: its window handles, its redraw waker and its paths.
pub struct AppCtx<'a> {
    pub(crate) redraw_requested: &'a mut bool,
    pub(crate) redraw_waker: Option<&'a RedrawWaker>,
    // Standard backend-agnostic types, for app code doing native platform integration. `None` on backends that cannot report them.
    pub(crate) raw_window_handle: Option<RawWindowHandle>,
    pub(crate) raw_display_handle: Option<RawDisplayHandle>,
}

impl<'a> AppCtx<'a> {
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    /// A `Send`/`Sync` handle to wake the UI loop from a background thread (see [`RedrawWaker`]). Grab it once (e.g. on the first frame) and hand clones to worker threads so their results are picked up promptly.
    pub fn redraw_waker(&self) -> Option<RedrawWaker> {
        self.redraw_waker.cloned()
    }

    /// This window's raw OS window handle (e.g. a Wayland `wl_surface`, an X11 window id), for app code that needs native platform integration the framework doesn't wrap — client-side drag-and-drop, IME, a custom GPU surface. `None` on backends that can't report it (headless, or before the surface exists).
    pub fn raw_window_handle(&self) -> Option<RawWindowHandle> {
        self.raw_window_handle
    }

    /// This window's raw OS display handle (e.g. the Wayland `wl_display`, the X11 `Display`). Pairs with [`raw_window_handle`](Self::raw_window_handle) for native platform integration. `None` where unavailable.
    pub fn raw_display_handle(&self) -> Option<RawDisplayHandle> {
        self.raw_display_handle
    }
}
