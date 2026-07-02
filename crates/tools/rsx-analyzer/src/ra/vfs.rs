use std::path::{Path, PathBuf};

use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_paths::AbsPathBuf;
use ra_ap_vfs::{FileId, VfsPath};

use super::EmbeddedAnalyzer;

impl EmbeddedAnalyzer {
    /// Whether `path` is a file the loaded workspace graph knows. `false` for a generated module added after load (a new `.rsx`), which signals a stale graph.
    pub fn knows_file(&self, path: &Path) -> bool {
        self.file_id(path).is_some()
    }

    /// Re-reads `path` from disk and overlays it into the analyzer. The LSP only serves `.rsx`, so it never receives `didChange` for hand-written `.rs` files — without this they stay frozen at load time, so go-to-def / diagnostics / find-refs that cross into real Rust go stale after any edit (e.g. renaming a fn whose definition lives in a `.rs`). Returns `false` if `path` is not in the loaded graph or can't be read, signalling the caller to reload the whole workspace.
    pub fn refresh_from_disk(&mut self, path: &Path) -> bool {
        let Some(file_id) = self.file_id(path) else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        self.overlay(file_id, text);
        true
    }

    /// Resolves a filesystem path to its `FileId`, if the file is part of the loaded workspace (true for a generated `.rs` reached via `#[path] mod`).
    pub(super) fn file_id(&self, path: &Path) -> Option<FileId> {
        // `AbsPathBuf` is UTF-8 + absolute: `to_str()` rejects non-UTF-8 and `try_from(&str)` rejects non-absolute, so neither path panics.
        let abs = AbsPathBuf::try_from(path.to_str()?).ok()?;
        self.vfs.file_id(&VfsPath::from(abs)).map(|(id, _)| id)
    }

    /// Resolves a `FileId` back to its filesystem path via the `Vfs` (the analyzer's authority on file_id↔path), for mapping navigation targets to LSP `Location`s.
    pub(super) fn file_path(&self, file_id: FileId) -> Option<PathBuf> {
        let abs = self.vfs.file_path(file_id).as_path()?;
        let path: &Path = abs.as_ref();
        Some(path.to_path_buf())
    }

    /// Overlays new text for an already-known `FileId`, synchronously updating the salsa inputs. The analysis snapshot is created only after this returns, so it never blocks `apply_change` (which waits for every live snapshot to drop).
    pub(super) fn overlay(&mut self, file_id: FileId, text: String) {
        let mut change = ChangeWithProcMacros::default();
        change.change_file(file_id, Some(text));
        self.host.apply_change(change);
    }
}
