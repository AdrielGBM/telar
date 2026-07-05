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

/// Parses the `[rsx]` table from `<package_root>/rsx.toml`, or `None` if the file/section is absent.
fn read_rsx_section(package_root: &Path) -> Option<toml::Table> {
    let content = std::fs::read_to_string(package_root.join("rsx.toml")).ok()?;
    content
        .parse::<toml::Table>()
        .ok()?
        .get("rsx")?
        .as_table()
        .cloned()
}

/// Reads `[rsx] auto_modules` from `rsx.toml`. A missing file or key yields `false`, so filesystem
/// module discovery is strictly opt-in.
pub fn auto_modules_enabled(package_root: &Path) -> bool {
    read_rsx_section(package_root)
        .and_then(|s| s.get("auto_modules")?.as_bool())
        .unwrap_or(false)
}

/// The directory that baked `src:"..."` asset paths resolve against: `[rsx] assets` in `rsx.toml`
/// (default `"assets"`), joined onto the package root — so assets live in one place (e.g. `./assets`)
/// regardless of which `.rsx` references them, instead of being tied to each `.rsx`'s own directory.
pub fn assets_root(package_root: &Path) -> PathBuf {
    let configured = read_rsx_section(package_root)
        .and_then(|s| s.get("assets")?.as_str().map(str::to_string))
        .unwrap_or_else(|| "assets".to_string());
    package_root.join(configured)
}

/// Emits `pub mod` declarations mirroring the `src_dir` tree for every hand-written `.rs` module, so
/// an app can rely on filesystem module discovery instead of hand-written `mod.rs`/`mod` statements.
/// Skips the crate roots (`lib.rs`, `main.rs`) and directories with no `.rs` under them (asset- or
/// markup-only dirs). A directory that has its own `mod.rs` is declared but not descended into, so it
/// stays hand-managed — the escape hatch for opting a subtree out of discovery. Each leaf module
/// carries an absolute `#[path]` so rust-analyzer resolves it from the macro expansion (a plain
/// file-based `mod` from a proc macro is not reliably linked to its file by the analyzer).
pub fn discover_rust_modules(src_dir: &Path) -> String {
    let mut out = String::new();
    emit_dir_modules(src_dir, &mut out);
    out
}

fn emit_dir_modules(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !crate::naming::is_ident(name) {
            continue;
        }
        if path.is_dir() {
            let mod_rs = path.join("mod.rs");
            if mod_rs.exists() {
                out.push_str(&mod_decl(name, &mod_rs));
            } else if dir_has_rust_module(&path) {
                out.push_str(&format!("pub mod {name} {{\n"));
                emit_dir_modules(&path, out);
                out.push_str("}\n");
            }
        } else if is_rust_module_file(&path) {
            out.push_str(&mod_decl(name, &path));
        }
    }
}

/// A `pub mod` declaration pinned to `file` via an absolute `#[path]`. `{:?}` renders the path as an escaped Rust string literal.
fn mod_decl(name: &str, file: &Path) -> String {
    format!("#[path = {:?}] pub mod {name};\n", file.to_string_lossy())
}

/// A `.rs` file that is a declarable module: not a crate root and not a `mod.rs` (the latter is the module root of its own directory, not a sibling child).
fn is_rust_module_file(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    !matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("lib.rs") | Some("main.rs") | Some("mod.rs")
    )
}

fn dir_has_rust_module(dir: &Path) -> bool {
    collect_files_by_ext(dir, "rs", &|_| true)
        .iter()
        .any(|p| is_rust_module_file(p) || p.file_name().and_then(|n| n.to_str()) == Some("mod.rs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_root_default_and_configured() {
        let root = std::env::temp_dir().join(format!("rsx_assets_root_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // No rsx.toml → default `assets`.
        assert_eq!(assets_root(&root), root.join("assets"));

        // `[rsx] assets = "..."` overrides the location (relative to the package root).
        std::fs::write(
            root.join("rsx.toml"),
            "[rsx]\nassets = \"src/shared/assets\"\n",
        )
        .unwrap();
        assert_eq!(assets_root(&root), root.join("src/shared/assets"));

        // A section without `assets` falls back to the default; `auto_modules` reads the same section.
        std::fs::write(root.join("rsx.toml"), "[rsx]\nauto_modules = true\n").unwrap();
        assert_eq!(assets_root(&root), root.join("assets"));
        assert!(auto_modules_enabled(&root));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_rust_modules_mirrors_tree() {
        let root =
            std::env::temp_dir().join(format!("rsx_discover_modules_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in [
            "core",
            "shared/components",
            "features/assets",
            "hand/nested",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for (p, body) in [
            ("lib.rs", ""),                     // crate root: skipped
            ("main.rs", ""),                    // crate root: skipped
            ("util.rs", ""),                    // top-level module
            ("core/app.rs", ""),                // nested module
            ("core/theme.rs", ""),              // nested module
            ("core/sidebar.rsx", ""),           // markup: ignored
            ("shared/demo.rs", ""),             // nested module
            ("shared/components/card.rsx", ""), // markup-only subdir: pruned
            ("features/home.rsx", ""),          // markup-only tree: pruned
            ("features/assets/x.png", ""),      // asset: pruned
            ("hand/mod.rs", ""),                // hand-managed: declared, not descended
            ("hand/nested/inner.rs", ""),
        ] {
            std::fs::write(root.join(p), body).unwrap();
        }

        let out = discover_rust_modules(&root);
        let _ = std::fs::remove_dir_all(&root);

        assert!(out.contains("pub mod util;"), "{out}");
        assert!(out.contains("pub mod core {"), "{out}");
        assert!(out.contains("pub mod app;"), "{out}");
        assert!(out.contains("pub mod theme;"), "{out}");
        assert!(out.contains("pub mod shared {"), "{out}");
        assert!(out.contains("pub mod demo;"), "{out}");
        // Hand-managed dir (has mod.rs) is declared but not descended into.
        assert!(out.contains("pub mod hand;"), "{out}");
        assert!(!out.contains("nested") && !out.contains("inner"), "{out}");
        // Markup/asset-only trees and crate roots produce nothing.
        assert!(
            !out.contains("features") && !out.contains("components"),
            "{out}"
        );
        assert!(!out.contains("home") && !out.contains("sidebar"), "{out}");
        assert!(
            !out.contains("pub mod lib") && !out.contains("pub mod main"),
            "{out}"
        );
    }
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
