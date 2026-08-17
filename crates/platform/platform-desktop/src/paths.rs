use std::path::PathBuf;

use services_core::AppPathsProvider;

pub struct DesktopPathsProvider;

impl AppPathsProvider for DesktopPathsProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::config_dir()
    }

    fn data_dir(&self) -> Option<PathBuf> {
        dirs::data_dir()
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        dirs::cache_dir()
    }

    fn state_dir(&self) -> Option<PathBuf> {
        dirs::state_dir().or_else(|| dirs::data_dir())
    }

    fn runtime_dir(&self) -> Option<PathBuf> {
        dirs::runtime_dir()
    }
}
