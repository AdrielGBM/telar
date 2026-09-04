//! Where an Android app's files live, resolved from the activity.

use std::path::PathBuf;

use android_activity::AndroidApp;
use services_core::AppPathsProvider;

/// The app's config, cache and data directories, resolved from the activity.
pub struct AndroidPathsProvider {
    app: AndroidApp,
}

impl AndroidPathsProvider {
    pub fn new(app: AndroidApp) -> Self {
        Self { app }
    }
}

impl AppPathsProvider for AndroidPathsProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        self.app.internal_data_path()
    }

    fn data_dir(&self) -> Option<PathBuf> {
        self.app.internal_data_path()
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.app.internal_data_path().map(|p| p.join("cache"))
    }

    fn system_fonts_dir(&self) -> Option<PathBuf> {
        Some(crate::fonts::system_fonts_dir())
    }

    fn sans_serif_candidates(&self) -> Vec<String> {
        crate::fonts::sans_serif_candidates()
    }
}
