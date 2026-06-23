//! Shared project-root resolution for the rsx toolchain.
//!
//! `cargo-rsx` (the dev/build CLI) and `rsx-analyzer` (the LSP) both need to
//! anchor file discovery — generated `.rsx/` output, `.rsx/lsp/`, theme
//! scanning — to the same directory. Keeping the upward-walk and the marker
//! definitions in one place stops the two tools from diverging.

use std::path::{Path, PathBuf};

/// Walks up from `start` (or its parent directory if `start` is a file),
/// returning the first ancestor directory for which `matches` returns true.
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

/// Nearest ancestor directory containing an `rsx.toml`.
pub fn find_rsx_root(start: &Path) -> Option<PathBuf> {
    find_ancestor_dir(start, |dir| dir.join("rsx.toml").exists())
}

/// Nearest ancestor directory whose `Cargo.toml` declares a `[workspace]` table.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    find_ancestor_dir(start, dir_is_workspace_root)
}

/// The canonical rsx project root: the `rsx.toml` directory, falling back to
/// the Cargo workspace root when no `rsx.toml` is present.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    find_rsx_root(start).or_else(|| find_workspace_root(start))
}

fn dir_is_workspace_root(dir: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return false;
    };
    content
        .parse::<toml::Table>()
        .map(|table| table.contains_key("workspace"))
        .unwrap_or(false)
}
