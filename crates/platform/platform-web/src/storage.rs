//! Preferences kept in the browser's `localStorage`, under keys namespaced by the app.

use services_core::PreferenceStore;

/// The page's `localStorage`, each key prefixed with the app's name so two apps on one origin keep apart.
///
/// Every call can fail — a private window, storage turned off, a quota reached, a sandboxed frame — and a failure is a preference kept for the run only: [`services_core::stored_preference`] still answers from what was set, through the memory beside it.
pub struct WebStorage {
    prefix: String,
    run: services_core::MemoryStore,
}

impl WebStorage {
    pub fn new(app_name: &str) -> Self {
        Self {
            prefix: format!("{app_name}:"),
            run: services_core::MemoryStore::default(),
        }
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }
}

impl PreferenceStore for WebStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.run.get(key).or_else(|| {
            Self::storage()?
                .get_item(&format!("{}{key}", self.prefix))
                .ok()
                .flatten()
        })
    }

    fn set(&self, key: &str, value: Option<&str>) {
        self.run.set(key, value);
        let Some(storage) = Self::storage() else {
            return;
        };
        let key = format!("{}{key}", self.prefix);
        let kept = match value {
            Some(value) => storage.set_item(&key, value),
            None => storage.remove_item(&key),
        };
        if kept.is_err() {
            tracing::debug!(
                key,
                "localStorage refused a preference; it holds for this visit only"
            );
        }
    }
}
