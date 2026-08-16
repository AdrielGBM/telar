//! Project-root resolution for the rsx toolchain.
//!
//! `cargo-telar` (the dev/build CLI) and `telar-analyzer` (the LSP) both anchor file discovery — generated
//! `.telar/` output, theme scanning — to the same directory. The upward walk and the marker definitions live
//! here so the two tools cannot diverge on where a project starts.

use std::path::{Path, PathBuf};

/// Walks up from `start` (or its parent directory if `start` is a file), returning the first ancestor
/// directory for which `matches` returns true.
pub fn find_ancestor_dir(start: &Path, matches: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    loop {
        if matches(dir) {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Nearest ancestor directory containing a `telar.toml`.
pub fn find_telar_root(start: &Path) -> Option<PathBuf> {
    find_ancestor_dir(start, |dir| dir.join("telar.toml").exists())
}

/// Nearest ancestor directory whose `Cargo.toml` declares a `[workspace]` table.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    find_ancestor_dir(start, is_workspace_root_dir)
}

fn is_workspace_root_dir(dir: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return false;
    };
    content
        .parse::<toml::Table>()
        .map(|table| table.contains_key("workspace"))
        .unwrap_or(false)
}
