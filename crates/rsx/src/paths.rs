use std::path::PathBuf;

use services_core::AppPathsProvider;

// `dirs` is excluded from Android targets via Cargo target-specific deps.
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
}
