//! The ambient services an application reaches for without naming a backend: the clipboard, file dialogs, the per-app directories and opening a URI.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod app_paths;
pub mod clipboard;
pub mod dialogs;
pub mod paths;
pub mod preferences;
#[cfg(feature = "di")]
mod registry;
#[cfg(feature = "di")]
mod scope;
pub mod uri;

pub use clipboard::{Clipboard, clipboard, clipboard_text, set_clipboard, set_clipboard_text};
pub use dialogs::{FileDialog, FileDialogs, FileFilter, file_dialogs, set_file_dialogs};
#[cfg(feature = "system-paths")]
pub use paths::SystemPaths;
pub use paths::{AppPathsProvider, NoPaths};
pub use preferences::{
    FileStore, MemoryStore, PreferenceStore, read_if_present, set_preference_store,
    store_preference, stored_preference, write_file_atomically,
};
#[cfg(feature = "di")]
pub use registry::ServiceError;
#[cfg(feature = "di")]
pub use scope::{Scope, context, provide, set_context, try_inject, with_service};
#[cfg(feature = "system-opener")]
pub use uri::SystemOpener;
pub use uri::{UriOpener, open_beside, open_uri, set_uri_opener, uri_opener};

#[cfg(all(test, feature = "di"))]
#[path = "lib_test.rs"]
mod tests;
