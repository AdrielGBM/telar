//! File-discovery utilities: walking a source tree for `.rsx`/`.rs` files and deriving their output stems and mirrored `.rs` paths.

use std::path::{Path, PathBuf};

/// Recursively collects files with `extension` under `dir`, descending into a subdirectory only when `keep_dir` returns true for its name. The result is sorted.
pub fn collect_files_by_ext(
    dir: &Path,
    extension: &str,
    keep_dir: &dyn Fn(&str) -> bool,
) -> Vec<PathBuf> {
    fn walk(
        dir: &Path,
        extension: &str,
        keep_dir: &dyn Fn(&str) -> bool,
        result: &mut Vec<PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| !keep_dir(name));
                if !skip {
                    walk(&path, extension, keep_dir, result);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                result.push(path);
            }
        }
    }

    let mut result = Vec::new();
    walk(dir, extension, keep_dir, &mut result);
    result.sort();
    result
}

pub fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(dir, "rsx", &|_| true)
}

/// Derives a unique stem for a `.rsx` file from its path relative to `src_dir`, flattening subdirectories with `_` so files in different directories don't collide (e.g. `src/components/button.rsx` -> `components_button`).
pub fn relative_stem(path: &Path, src_dir: &Path) -> String {
    let rel = path.strip_prefix(src_dir).unwrap_or(path);
    let without_ext = rel.with_extension("");
    without_ext
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("_")
}

/// Derives the output `.rs` path (relative to the output root) for a `.rsx` file by mirroring its location under `src_dir`, so files in different directories never collide (e.g. `src/components/button.rsx` -> `components/button.rs`). Used for the transpiler's `.rsx/build/` output. Returns `None` for files outside `src_dir`: those are never transpiled, so they have no place in the build tree — and flattening their absolute path would escape the output root entirely.
pub fn relative_output_path(path: &Path, src_dir: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(src_dir).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.with_extension("rs"))
}
