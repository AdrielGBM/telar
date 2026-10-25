//! What a person chose, kept between runs: a colour scheme, a language, whatever the application declares.

use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Where preferences are kept, as text under keys the application and Telar name.
///
/// Each runner installs its target's: a file in the app's config directory on the desktop, a terminal and Android, the browser's `localStorage` on the web. A store that cannot keep something — a private window, storage turned off — keeps it for the run and says nothing: a preference that does not survive is a lesser app, not a broken one.
pub trait PreferenceStore: Send + Sync + 'static {
    fn get(&self, key: &str) -> Option<String>;

    /// Keeps `value` under `key`, or forgets the key for `None`.
    fn set(&self, key: &str, value: Option<&str>);
}

reactive_local::released_slot! {
    static STORE: Arc<dyn PreferenceStore>;
}

/// Installs the store preferences are kept in. Each runner installs its target's; a later call replaces it.
pub fn set_preference_store(store: Arc<dyn PreferenceStore>) {
    STORE.replace(store);
}

/// The installed store, or one in memory where none was installed — headless, a test, a preview — so a preference still holds for the run.
pub fn preference_store() -> Arc<dyn PreferenceStore> {
    STORE.get_or_insert_with(|| Arc::new(MemoryStore::default()))
}

/// The preference kept under `key`, if any.
pub fn stored_preference(key: &str) -> Option<String> {
    preference_store().get(key)
}

/// Keeps `value` under `key` for the next run, or forgets it for `None`.
pub fn store_preference(key: &str, value: Option<&str>) {
    preference_store().set(key, value);
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

/// Writes `contents` to `path` through a file of its own beside it renamed over it, so a run that dies mid-write leaves the previous file rather than half of one, and two writers never share the file they write through.
pub fn write_file_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let beside = path.with_file_name(name);
    let written = std::fs::write(&beside, contents).and_then(|()| std::fs::rename(&beside, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&beside);
    }
    written
}

/// The text of the file at `path`, or `None` where there is no such file. Any other failure, an unreadable or non-text file, is an `Err`: a caller that saves over the file afterwards must not, since what it could not read it cannot have kept.
pub fn read_if_present(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Preferences kept in one file, a line per key: the key, a tab, the value, with tabs, newlines and backslashes escaped.
///
/// Read once when opened and written whole on every change that changes something, through [`write_file_atomically`]. A file that exists but cannot be read is left alone: the store keeps what is set for the run and never writes, so the file is not replaced by what little this run knows.
pub struct FileStore {
    path: PathBuf,
    values: Mutex<BTreeMap<String, String>>,
    writable: bool,
}

impl FileStore {
    /// The store kept at `path`, with what it already holds. A file that does not exist is an empty store; one that cannot be read or is not text is logged and kept for the run only.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (values, writable) = match read_if_present(&path) {
            Ok(text) => (text.map_or_else(BTreeMap::new, |text| parse(&text)), true),
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "preferences could not be read; they will not be saved over the file");
                (BTreeMap::new(), false)
            }
        };
        Self {
            path,
            values: Mutex::new(values),
            writable,
        }
    }

    fn write(&self, values: &BTreeMap<String, String>) -> std::io::Result<()> {
        write_file_atomically(&self.path, render(values).as_bytes())
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
        if changed
            && self.writable
            && let Err(error) = self.write(&values)
        {
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
