//! The files a running session names itself in, taken back when it is interrupted: Ctrl-C, a `kill`, or the terminal it runs in closing ends the process without running a destructor, and a file left naming a session that is gone tells the editor, and the next session, that it still runs.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, PoisonError};

/// The status a shell reports for a command ended by Ctrl-C.
const INTERRUPTED: i32 = 130;

static LEFTOVERS: Leftovers = Leftovers::new();

/// Removes `path` if the process is interrupted before [`keep_on_interrupt`] takes it back. The first call puts the handler in place, so a command that names no file keeps the default behaviour.
pub(crate) fn remove_on_interrupt(path: &Path) {
    LEFTOVERS.add(path);
    static HANDLER: Once = Once::new();
    HANDLER.call_once(|| {
        let installed = ctrlc::set_handler(|| {
            LEFTOVERS.remove_all();
            std::process::exit(INTERRUPTED);
        });
        if let Err(e) = installed {
            eprintln!("[cargo-telar] could not watch for interruptions: {e}");
        }
    });
}

/// Leaves `path` alone on an interruption, for a session that has already removed it.
pub(crate) fn keep_on_interrupt(path: &Path) {
    LEFTOVERS.keep(path);
}

/// The files to remove on an interruption.
struct Leftovers(Mutex<Vec<PathBuf>>);

impl Leftovers {
    const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    fn add(&self, path: &Path) {
        self.paths().push(path.to_path_buf());
    }

    fn keep(&self, path: &Path) {
        self.paths().retain(|held| held != path);
    }

    fn remove_all(&self) {
        for path in std::mem::take(&mut *self.paths()) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[cfg(test)]
    fn holds(&self, path: &Path) -> bool {
        self.paths().iter().any(|held| held == path)
    }

    // The handler runs on a thread of its own and must still find the list after a panic elsewhere.
    fn paths(&self) -> std::sync::MutexGuard<'_, Vec<PathBuf>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whether an interruption now would remove `path`.
#[cfg(test)]
pub(crate) fn removes_on_interrupt(path: &Path) -> bool {
    LEFTOVERS.holds(path)
}

#[cfg(test)]
#[path = "interrupt_test.rs"]
mod tests;
