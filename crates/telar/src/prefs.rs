//! User preferences persisted between runs: the chosen renderer, and where the app last was.

use serde::{Deserialize, Serialize};
use services_core::AppPathsProvider;
use std::path::PathBuf;

use crate::config::RendererBackend;
#[derive(Serialize, Deserialize, Clone, Default)]
/// What is remembered between runs: the chosen renderer, and where the app last was.
pub struct UserPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<RendererBackend>,
    /// The app's history when it last moved, root-first, each entry as `LocationFormat::root` writes it, its locale included. Kept by the desktop and terminal runners, which reopen it when launched without `--location`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<String>,
    #[serde(skip)]
    pub(crate) unreadable: bool,
}

impl UserPrefs {
    /// What the file holds. One that exists but cannot be read or parsed is logged and left alone: the defaults serve the run and [`save`](Self::save) refuses to replace it.
    pub fn load(app_name: &str, paths: &dyn AppPathsProvider) -> Self {
        let Some(path) = prefs_path(app_name, paths) else {
            return Self::default();
        };
        let parsed = services_core::read_if_present(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                text.map_or_else(
                    || Ok(Self::default()),
                    |text| toml::from_str(&text).map_err(|e| e.to_string()),
                )
            });
        parsed.unwrap_or_else(|error| {
            tracing::error!(path = %path.display(), %error, "preferences could not be read; they will not be saved over the file");
            Self {
                unreadable: true,
                ..Self::default()
            }
        })
    }

    pub fn save(&self, app_name: &str, paths: &dyn AppPathsProvider) -> Result<(), String> {
        if self.unreadable {
            return Err("the stored preferences could not be read and are kept as they are".into());
        }
        let path =
            prefs_path(app_name, paths).ok_or_else(|| "Cannot resolve config dir".to_string())?;

        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        services_core::write_file_atomically(&path, content.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prefs_path(app_name: &str, paths: &dyn AppPathsProvider) -> Option<PathBuf> {
    paths
        .config_dir()
        .map(|base| base.join(app_name).join("prefs.toml"))
}

/// A browser keeps no files, and asking the provider behind its `dyn` cannot show the compiler that, which then links the TOML codec into every page.
#[cfg(target_arch = "wasm32")]
fn prefs_path(_app_name: &str, _paths: &dyn AppPathsProvider) -> Option<PathBuf> {
    None
}

#[cfg(test)]
#[path = "prefs_test.rs"]
mod tests;
