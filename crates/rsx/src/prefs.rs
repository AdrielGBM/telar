use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::RendererBackend;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct UserPrefs {
    #[serde(default)]
    pub renderer: RendererPrefs,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct RendererPrefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<RendererBackend>,
}

impl UserPrefs {
    pub fn load(app_name: &str) -> Self {
        let Some(path) = prefs_path(app_name) else {
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

    pub fn save(&self, app_name: &str) -> Result<(), String> {
        let path = prefs_path(app_name).ok_or_else(|| "Cannot resolve config dir".to_string())?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
}

fn prefs_path(app_name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|base| base.join(app_name).join("prefs.toml"))
}
