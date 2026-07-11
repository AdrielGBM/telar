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

/// Declares `pub mod` for every hand-written `.rs` module mirroring the `src_dir` tree, so an app can rely
/// on filesystem module discovery instead of hand-written `mod.rs`/`mod` statements. Returns the top-level
/// declarations and, for each discovered subdirectory, writes a generated file under `modtree_dir` holding
/// that directory's children. Skips the crate roots (`lib.rs`, `main.rs`) and directories with no `.rs` under
/// them (asset- or markup-only dirs). A directory that has its own `mod.rs` is declared but not descended
/// into, so it stays hand-managed — the escape hatch for opting a subtree out of discovery.
///
/// Every module — top-level file, subdirectory, and the children inside each generated file — is a *file-based*
/// `#[path] pub mod` (the exact shape the `.rsx` build files use). It deliberately never emits an inline
/// `mod dir { … }` block: rust-analyzer mis-resolves a `#[path]` attribute on a module nested inside an inline
/// block produced by a proc macro, string-joining the inline module's name onto the child's already-absolute
/// path (`core//abs/core/app.rs`) and failing to find it (E0583). rustc joins those pieces with real path
/// semantics, so the absolute child path wins and it compiles — which is why the two disagreed. Routing every
/// directory through a real generated file (`#[path = "…/core.rs"] pub mod core;`, its children flat inside
/// that file) keeps the analyzer and the compiler in step.
pub fn discover_rust_modules(src_dir: &Path, modtree_dir: &Path) -> std::io::Result<String> {
    let mut out = String::new();
    emit_children(src_dir, "", modtree_dir, &mut out)?;
    Ok(out)
}

/// Appends the `#[path] pub mod` declarations for the direct children of `dir` to `out`. A subdirectory's own
/// children are written to a generated file under `modtree_dir` (named by the flattened module path, e.g.
/// `core__widgets.rs`, so sibling directories never collide) that the emitted `pub mod` then points at.
fn emit_children(
    dir: &Path,
    flat_prefix: &str,
    modtree_dir: &Path,
    out: &mut String,
) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
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
                let flat = if flat_prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{flat_prefix}__{name}")
                };
                let mut body = String::new();
                emit_children(&path, &flat, modtree_dir, &mut body)?;
                let gen_file = modtree_dir.join(format!("{flat}.rs"));
                write_if_changed(&gen_file, &body)?;
                out.push_str(&mod_decl(name, &gen_file));
            }
        } else if is_rust_module_file(&path) {
            out.push_str(&mod_decl(name, &path));
        }
    }
    Ok(())
}

/// Writes `content` to `path` only when it differs, to avoid retriggering recompilation on unchanged output.
fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    let stale = std::fs::read_to_string(path)
        .map(|existing| existing != content)
        .unwrap_or(true);
    if stale {
        std::fs::write(path, content)?;
    }
    Ok(())
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

        let modtree = root.join("__modules");
        std::fs::create_dir_all(&modtree).unwrap();
        let out = discover_rust_modules(&root, &modtree).unwrap();
        let core_rs = std::fs::read_to_string(modtree.join("core.rs")).unwrap_or_default();
        let shared_rs = std::fs::read_to_string(modtree.join("shared.rs")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&root);

        // Every module is a file-based `#[path] pub mod` — never an inline `mod dir { … }` block (which breaks rust-analyzer's `#[path]` resolution).
        assert!(!out.contains('{'), "no inline module blocks:\n{out}");
        assert!(out.contains("pub mod util;"), "{out}");
        // A subdirectory is a file-based module pointing at its generated tree file; its children live inside that file.
        assert!(out.contains("pub mod core;"), "{out}");
        assert!(core_rs.contains("pub mod app;"), "{core_rs}");
        assert!(core_rs.contains("pub mod theme;"), "{core_rs}");
        assert!(out.contains("pub mod shared;"), "{out}");
        assert!(shared_rs.contains("pub mod demo;"), "{shared_rs}");
        // Hand-managed dir (has mod.rs) is declared but not descended into.
        assert!(out.contains("pub mod hand;"), "{out}");
        assert!(
            !out.contains("nested") && !out.contains("inner") && !core_rs.contains("nested"),
            "{out}"
        );
        // Markup/asset-only trees and crate roots produce nothing.
        assert!(
            !out.contains("features") && !core_rs.contains("components"),
            "{out}\n---\n{core_rs}"
        );
        assert!(
            !out.contains("home") && !core_rs.contains("sidebar"),
            "{out}"
        );
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
