use std::path::PathBuf;

use android_activity::AndroidApp;
use services_core::AppPathsProvider;

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
}
