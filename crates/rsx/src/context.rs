use crate::config::RendererBackend;
use crate::prefs::UserPrefs;

pub struct AppContext<'a> {
    pub(crate) app_name: &'a str,
    pub(crate) prefs: &'a mut UserPrefs,
    pub(crate) pending_restart: &'a mut bool,
    pub(crate) redraw_requested: &'a mut bool,
}

impl<'a> AppContext<'a> {
    pub fn request_redraw(&mut self) {
        *self.redraw_requested = true;
    }

    pub fn restart_required(&self) -> bool {
        *self.pending_restart
    }

    pub fn renderer_backend(&self) -> Option<RendererBackend> {
        self.prefs.renderer.backend
    }

    pub fn set_renderer_backend(&mut self, backend: RendererBackend) {
        self.prefs.renderer.backend = Some(backend);
        *self.pending_restart = true;
        if let Err(e) = self.prefs.save(self.app_name) {
            tracing::warn!("Could not save preferences: {e}");
        }
    }

    pub fn reset_renderer_backend(&mut self) {
        self.prefs.renderer.backend = None;
        *self.pending_restart = true;
        if let Err(e) = self.prefs.save(self.app_name) {
            tracing::warn!("Could not save preferences: {e}");
        }
    }
}
