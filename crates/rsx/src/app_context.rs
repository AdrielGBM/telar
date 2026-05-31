use services_core::AppPathsProvider;

use crate::config::RendererBackend;
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

pub struct AppCtx<'a> {
    pub(crate) app_name: &'a str,
    pub(crate) prefs: &'a mut UserPrefs,
    pub(crate) paths: &'a dyn AppPathsProvider,
    pub(crate) pending_restart: &'a mut bool,
    pub(crate) redraw_requested: &'a mut bool,
    pub(crate) window_signals: Option<&'a WindowSignals>,
}

impl<'a> AppCtx<'a> {
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    pub fn window(&self) -> Option<&WindowSignals> {
        self.window_signals
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
