//! Native file dialogs.
//!
//! Every dialog here BLOCKS the thread it runs on until the user answers — a portal round-trip on Linux, a modal panel elsewhere — so none of them runs on the UI thread. Each spawns the chooser on a worker and hands the answer back through [`reactive_core::spawn_task`], which delivers on the UI thread with the reactive runtime already entered: the callback can write a signal directly, and the frame that follows shows it.
//!
//! The returned [`Task`] cancels the callback (not the dialog — the OS owns that window once it is up), which is what a surface tearing down while a dialog is open wants.

use std::path::PathBuf;

use reactive_core::{Task, spawn_task};
use services_core::file_dialogs;
pub use services_core::{FileDialog, FileFilter};

/// Runs `pick` on a worker and delivers its answer to `on_done` on the UI thread. With no chooser installed (headless, Android) the answer is the empty one, delivered the same way — a caller never has to branch on whether the platform has dialogs.
fn ask<T, P, F>(pick: P, on_done: F) -> Task
where
    T: Default + Send + 'static,
    P: FnOnce(&dyn services_core::FileDialogs) -> T + Send + 'static,
    F: FnOnce(T) + 'static,
{
    spawn_task(
        move || match file_dialogs() {
            Some(dialogs) => pick(dialogs.as_ref()),
            None => T::default(),
        },
        on_done,
    )
}

/// Asks for one existing file. `None` means the user cancelled.
pub fn open_file(request: FileDialog, on_done: impl FnOnce(Option<PathBuf>) + 'static) -> Task {
    ask(move |d| d.open_file(request), on_done)
}

/// Asks for any number of existing files. An empty vec means the user cancelled.
pub fn open_files(request: FileDialog, on_done: impl FnOnce(Vec<PathBuf>) + 'static) -> Task {
    ask(move |d| d.open_files(request), on_done)
}

/// Asks where to write a file, confirming an overwrite if the platform does. `None` means cancelled.
pub fn save_file(request: FileDialog, on_done: impl FnOnce(Option<PathBuf>) + 'static) -> Task {
    ask(move |d| d.save_file(request), on_done)
}

/// Asks for a directory. `None` means the user cancelled.
pub fn pick_folder(request: FileDialog, on_done: impl FnOnce(Option<PathBuf>) + 'static) -> Task {
    ask(move |d| d.pick_folder(request), on_done)
}
