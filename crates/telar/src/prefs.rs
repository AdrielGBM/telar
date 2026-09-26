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
    /// The app's history when it last moved, root-first, each entry as `LocationFormat::root` writes it. Kept by the desktop and terminal runners, which reopen it when launched without `--location`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<String>,
}

impl UserPrefs {
    pub fn load(app_name: &str, paths: &dyn AppPathsProvider) -> Self {
        let Some(path) = prefs_path(app_name, paths) else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, app_name: &str, paths: &dyn AppPathsProvider) -> Result<(), String> {
        let path =
            prefs_path(app_name, paths).ok_or_else(|| "Cannot resolve config dir".to_string())?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
}

fn prefs_path(app_name: &str, paths: &dyn AppPathsProvider) -> Option<PathBuf> {
    paths
        .config_dir()
        .map(|base| base.join(app_name).join("prefs.toml"))
}
