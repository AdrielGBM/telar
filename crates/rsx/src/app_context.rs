use std::sync::Arc;

use services_core::AppPathsProvider;

use crate::config::RendererBackend;
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

/// A `Send`/`Sync`, cloneable handle that wakes the UI event loop (requests a redraw). Hand it to a background
/// thread (a file dialog, a `notify` watcher, a network fetch) so that when the thread has produced a result —
/// delivered over a channel — it can wake the loop; the app then drains the channel in [`crate::App::on_frame`]
/// and writes the result into signals. This is the bridge between off-thread work and the single-threaded
/// reactive/UI world (signals are `!Send`, so the data itself must cross via a channel, not a signal).
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

pub struct AppCtx<'a> {
    pub(crate) app_name: &'a str,
    pub(crate) prefs: &'a mut UserPrefs,
    pub(crate) paths: &'a dyn AppPathsProvider,
    pub(crate) pending_restart: &'a mut bool,
    pub(crate) redraw_requested: &'a mut bool,
    pub(crate) window_signals: Option<&'a WindowSignals>,
    pub(crate) redraw_waker: Option<&'a RedrawWaker>,
}

impl<'a> AppCtx<'a> {
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    pub fn window(&self) -> Option<&WindowSignals> {
        self.window_signals
    }

    /// A `Send`/`Sync` handle to wake the UI loop from a background thread (see [`RedrawWaker`]). Grab it once
    /// (e.g. on the first frame) and hand clones to worker threads so their results are picked up promptly.
    pub fn redraw_waker(&self) -> Option<RedrawWaker> {
        self.redraw_waker.cloned()
    }

    pub fn restart_required(&self) -> bool {
        *self.pending_restart
    }

    pub fn renderer_backend(&self) -> Option<RendererBackend> {
        self.prefs.backend
    }

    pub fn set_renderer_backend(&mut self, backend: RendererBackend) {
        self.prefs.backend = Some(backend);
        *self.pending_restart = true;
        if let Err(e) = self.prefs.save(self.app_name, self.paths) {
            tracing::warn!("Could not save preferences: {e}");
        }
    }

    pub fn reset_renderer_backend(&mut self) {
        self.prefs.backend = None;
        *self.pending_restart = true;
        if let Err(e) = self.prefs.save(self.app_name, self.paths) {
            tracing::warn!("Could not save preferences: {e}");
        }
    }
}
