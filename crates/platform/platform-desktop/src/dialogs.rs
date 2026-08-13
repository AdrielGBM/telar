use std::path::PathBuf;
use std::sync::Arc;

use services_core::{FileDialog, FileDialogs};

/// The OS file chooser, via `rfd`: the XDG portal on Linux, the native panel on macOS and Windows.
pub struct DesktopFileDialogs;

impl DesktopFileDialogs {
    /// Installs this backend as the process's file chooser. Called by the desktop runner at startup.
    pub fn install() {
        services_core::set_file_dialogs(Arc::new(DesktopFileDialogs));
    }
}

fn build(request: FileDialog) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    if let Some(dir) = request.directory {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = request.file_name {
        dialog = dialog.set_file_name(name);
    }
    for filter in request.filters {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(filter.name, &extensions);
    }
    dialog
}

impl FileDialogs for DesktopFileDialogs {
    fn open_file(&self, request: FileDialog) -> Option<PathBuf> {
        build(request).pick_file()
    }

    fn open_files(&self, request: FileDialog) -> Vec<PathBuf> {
        build(request).pick_files().unwrap_or_default()
    }

    fn save_file(&self, request: FileDialog) -> Option<PathBuf> {
        build(request).save_file()
    }

    fn pick_folder(&self, request: FileDialog) -> Option<PathBuf> {
        build(request).pick_folder()
    }
}
