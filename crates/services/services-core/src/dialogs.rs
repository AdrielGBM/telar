use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// One entry of a dialog's type filter: a name the user reads and the extensions it accepts.
#[derive(Clone, Debug, Default)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl FileFilter {
    pub fn new(name: impl Into<String>, extensions: &[&str]) -> Self {
        Self {
            name: name.into(),
            extensions: extensions.iter().map(|e| (*e).to_string()).collect(),
        }
    }
}

/// What to ask the OS for. Every field is optional: an empty request opens the platform's default
/// "any file, last directory" dialog.
#[derive(Clone, Debug, Default)]
pub struct FileDialog {
    pub title: Option<String>,
    pub directory: Option<PathBuf>,
    /// Pre-filled name, for a save dialog.
    pub file_name: Option<String>,
    pub filters: Vec<FileFilter>,
}

impl FileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    pub fn filter(mut self, name: impl Into<String>, extensions: &[&str]) -> Self {
        self.filters.push(FileFilter::new(name, extensions));
        self
    }
}

/// The OS file-chooser, as the vocabulary crate sees it.
///
/// Every method BLOCKS until the user answers, which is why the whole trait is `Send + Sync`: the caller
/// runs it on a worker thread and takes the answer back on the UI thread. Nothing here touches the event
/// loop, so a backend is free to be a portal call, a native panel, or a stub in a test.
pub trait FileDialogs: Send + Sync + 'static {
    fn open_file(&self, request: FileDialog) -> Option<PathBuf>;
    fn open_files(&self, request: FileDialog) -> Vec<PathBuf>;
    fn save_file(&self, request: FileDialog) -> Option<PathBuf>;
    fn pick_folder(&self, request: FileDialog) -> Option<PathBuf>;
}

static DIALOGS: OnceLock<Arc<dyn FileDialogs>> = OnceLock::new();

/// Installs the backend the app's dialogs go through. The desktop runner calls this at startup; a test or
/// a headless build can install a stub instead. The first call wins.
pub fn set_file_dialogs(provider: Arc<dyn FileDialogs>) {
    let _ = DIALOGS.set(provider);
}

/// The installed backend, or `None` on a platform with no file chooser (headless, Android today).
pub fn file_dialogs() -> Option<Arc<dyn FileDialogs>> {
    DIALOGS.get().cloned()
}
