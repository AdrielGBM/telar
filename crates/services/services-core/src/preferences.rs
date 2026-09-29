//! What a person chose, kept between runs: a colour scheme, a language, whatever the application declares.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

/// Where preferences are kept, as text under keys the application and Telar name.
///
/// Each runner installs its target's: a file in the app's config directory on the desktop, a terminal and Android, the browser's `localStorage` on the web. A store that cannot keep something — a private window, storage turned off — keeps it for the run and says nothing: a preference that does not survive is a lesser app, not a broken one.
pub trait PreferenceStore: Send + Sync + 'static {
    fn get(&self, key: &str) -> Option<String>;

    /// Keeps `value` under `key`, or forgets the key for `None`.
    fn set(&self, key: &str, value: Option<&str>);
}

static STORE: RwLock<Option<Arc<dyn PreferenceStore>>> = RwLock::new(None);

/// Installs the store preferences are kept in. Each runner installs its target's; a later call replaces it.
pub fn set_preference_store(store: Arc<dyn PreferenceStore>) {
    *STORE.write().unwrap_or_else(|e| e.into_inner()) = Some(store);
}

/// The installed store, or one in memory where none was installed — headless, a test, a preview — so a preference still holds for the run.
fn store() -> Arc<dyn PreferenceStore> {
    if let Some(store) = STORE.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return store;
    }
    let mut slot = STORE.write().unwrap_or_else(|e| e.into_inner());
    slot.get_or_insert_with(|| Arc::new(MemoryStore::default()))
        .clone()
}

/// The preference kept under `key`, if any.
pub fn stored_preference(key: &str) -> Option<String> {
    store().get(key)
}

/// Keeps `value` under `key` for the next run, or forgets it for `None`.
pub fn store_preference(key: &str, value: Option<&str>) {
    store().set(key, value);
}

/// Preferences kept for the run and no longer.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl PreferenceStore for MemoryStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    fn set(&self, key: &str, value: Option<&str>) {
        let mut values = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match value {
            Some(value) => values.insert(key.to_owned(), value.to_owned()),
            None => values.remove(key),
        };
    }
}

/// Preferences kept in one file, a line per key: the key, a tab, the value, with tabs, newlines and backslashes escaped.
///
/// Read once when opened and written whole on every change that changes something, through a file beside it renamed over it, so a run that dies mid-write leaves the previous file rather than half of one.
pub struct FileStore {
    path: PathBuf,
    values: Mutex<BTreeMap<String, String>>,
}

impl FileStore {
    /// The store kept at `path`, with what it already holds. A file that cannot be read is an empty store.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let values = std::fs::read_to_string(&path)
            .map(|text| parse(&text))
            .unwrap_or_default();
        Self {
            path,
            values: Mutex::new(values),
        }
    }

    fn write(&self, values: &BTreeMap<String, String>) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let beside = self.path.with_extension("tmp");
        std::fs::write(&beside, render(values))?;
        std::fs::rename(&beside, &self.path)
    }
}

impl PreferenceStore for FileStore {
    fn get(&self, key: &str) -> Option<String> {
        self.values
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    fn set(&self, key: &str, value: Option<&str>) {
        let mut values = self.values.lock().unwrap_or_else(|e| e.into_inner());
        let changed = match value {
            Some(value) => {
                values.insert(key.to_owned(), value.to_owned()).as_deref() != Some(value)
            }
            None => values.remove(key).is_some(),
        };
        if changed && let Err(error) = self.write(&values) {
            tracing::warn!(path = %self.path.display(), %error, "preferences were not saved");
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            },
            (c, false) => out.push(c),
        }
    }
    out
}

fn render(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{}\t{}\n", escape(key), escape(value)))
        .collect()
}

fn parse(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(key, value)| (unescape(key), unescape(value)))
        .collect()
}

#[cfg(test)]
#[path = "preferences_test.rs"]
mod tests;
