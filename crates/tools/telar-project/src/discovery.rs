//! File-discovery utilities: walking a source tree for `.rsx`/`.rs` files and deriving their output stems and mirrored `.rs` paths.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Recursively collects files with any of `extensions` under `root`, descending into a subdirectory only when `keep_dir` returns true for its name. The result is sorted.
///
/// A `root` that names a file is that file, if it matches — so a caller taking paths from a command line does not need a second walk for the case where the user pointed at one thing.
pub fn collect_files_by_ext(
    root: &Path,
    extensions: &[&str],
    keep_dir: &dyn Fn(&str) -> bool,
) -> Vec<PathBuf> {
    fn matches(path: &Path, extensions: &[&str]) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
    }
    fn walk(
        dir: &Path,
        extensions: &[&str],
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
                    walk(&path, extensions, keep_dir, result);
                }
            } else if matches(&path, extensions) {
                result.push(path);
            }
        }
    }

    if root.is_file() {
        return match matches(root, extensions) {
            true => vec![root.to_path_buf()],
            false => Vec::new(),
        };
    }
    let mut result = Vec::new();
    walk(root, extensions, keep_dir, &mut result);
    result.sort();
    result
}

/// Every `.rsx` under `dir`, recursively.
pub fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(dir, &["rsx"], &|_| true)
}

/// Every `.rsx` under a whole workspace, skipping what a build produced rather than a person: `target/` and any dot-directory (which is where the generated `.telar/` tree lives). [`find_rsx_files`] descends into everything, which is right for one crate's `src/` and ruinous one directory up — a workspace root holds a `target/` that dwarfs the sources, and this runs on every completion request.
pub fn find_rsx_files_in_tree(root: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(root, &["rsx"], &|name| {
        name != "target" && !name.starts_with('.')
    })
}

/// Reads `[telar] auto_modules`. A missing file or key yields `false`, so filesystem module discovery is strictly opt-in.
pub fn auto_modules_enabled(package_root: &Path) -> bool {
    crate::TelarManifest::load_or_default(package_root)
        .telar
        .auto_modules
}

/// The directory that baked `src:"..."` asset paths resolve against: `[telar] assets` in `telar.toml` (default `"assets"`), joined onto the package root — so assets live in one place (e.g. `./assets`) regardless of which `.rsx` references them, instead of being tied to each `.rsx`'s own directory.
pub fn assets_root(package_root: &Path) -> PathBuf {
    crate::TelarManifest::load_or_default(package_root)
        .telar
        .assets_root(package_root)
}

/// Declares `pub mod` for every hand-written `.rs` module mirroring the `src_dir` tree, so an app can rely on filesystem module discovery instead of hand-written `mod.rs`/`mod` statements. Returns the top-level declarations and, for each discovered subdirectory, writes a generated file under `modtree_dir` holding that directory's children. Skips the crate roots (`lib.rs`, `main.rs`) and directories with no `.rs` under them (asset- or markup-only dirs). A directory that has its own `mod.rs` is declared but not descended into, so it stays hand-managed — the escape hatch for opting a subtree out of discovery.
///
/// Every module — top-level file, subdirectory, and the children inside each generated file — is a *file-based* `#[path] pub mod` (the exact shape the `.rsx` build files use). It deliberately never emits an inline `mod dir { … }` block: rust-analyzer mis-resolves a `#[path]` attribute on a module nested inside an inline block produced by a proc macro, string-joining the inline module's name onto the child's already-absolute path (`core//abs/core/app.rs`) and failing to find it (E0583). rustc joins those pieces with real path semantics, so the absolute child path wins and it compiles — which is why the two disagreed. Routing every directory through a real generated file (`#[path = "…/core.rs"] pub mod core;`, its children flat inside that file) keeps the analyzer and the compiler in step.
///
/// Returns the declarations alongside every generated file path it wrote under `modtree_dir`, so a caller that prunes stale build output can tell these apart from an orphaned directory's leftover file.
///
/// **`from_dir` is where the macro was invoked, which is not always `src_dir`.** A module declares its own children, so `rsx_modules!()` in `app/editor/mod.rs` places the `.rsx` files of `app/editor` — declaring `pub mod app;` there instead would name an ancestor of the file doing the declaring, which is a cycle. The generated `.rs` a `.rsx` compiles to still mirrors the whole `src` tree, so the walk carries the prefix from `src_dir` even when it starts below it.
pub fn discover_rust_modules(
    src_dir: &Path,
    from_dir: &Path,
    modtree_dir: &Path,
    generated_dir: &Path,
    auto_modules: bool,
) -> std::io::Result<(String, Vec<PathBuf>)> {
    let prefix = from_dir
        .strip_prefix(src_dir)
        .unwrap_or(Path::new(""))
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("__");
    // A nested invocation is already inside a `mod.rs` that wrote its own `mod` statements, so re-declaring them here would redefine each.
    let discover = auto_modules && from_dir == src_dir;
    let mut out = String::new();
    let mut written = Vec::new();
    emit_children(
        from_dir,
        &prefix,
        modtree_dir,
        generated_dir,
        discover,
        &mut out,
        &mut written,
    )?;
    Ok((out, written))
}

/// Appends the `#[path] pub mod` declarations for the direct children of `dir` to `out`. A subdirectory's own children are written to a generated file under `modtree_dir` (named by the flattened module path, e.g. `core__widgets.rs`, so sibling directories never collide) that the emitted `pub mod` then points at.
fn emit_children(
    dir: &Path,
    flat_prefix: &str,
    modtree_dir: &Path,
    generated_dir: &Path,
    auto_modules: bool,
    out: &mut String,
    written: &mut Vec<PathBuf>,
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
                // A `mod.rs` places its own `.rsx` children by invoking `rsx_modules!()`, the only place they can be declared from — an outside module cannot add items to one. Saying so beats a "cannot find" about a file that is plainly there.
                if dir_has_rsx(&path) && !mod_rs_places_its_own(&mod_rs) {
                    let _ = write!(
                        out,
                        "compile_error!(\"{} holds `.rsx` files and a `mod.rs`, so only that file can place them: add `telar::rsx_modules!();` to it\");\n",
                        path.display()
                    );
                }
                if auto_modules {
                    out.push_str(&mod_decl(name, &mod_rs));
                }
            } else if dir_has_rust_module(&path) && (auto_modules || dir_has_rsx(&path)) {
                let flat = if flat_prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{flat_prefix}__{name}")
                };
                let mut body = String::new();
                emit_children(
                    &path,
                    &flat,
                    modtree_dir,
                    generated_dir,
                    auto_modules,
                    &mut body,
                    written,
                )?;
                let gen_file = modtree_dir.join(format!("{flat}.rs"));
                write_if_changed(&gen_file, &body)?;
                written.push(gen_file.clone());
                out.push_str(&mod_decl(name, &gen_file));
            }
        } else if is_rust_module_file(&path) {
            if auto_modules {
                out.push_str(&mod_decl(name, &path));
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rsx") {
            // A `.rsx` is a module where the file sits, so `shared/components/card.rsx` is `crate::shared::components::card`. Flattening to the crate root meant two files could not share a basename.
            out.push_str(&mod_decl(
                name,
                &rsx_output(generated_dir, flat_prefix, name),
            ));
        }
    }
    Ok(())
}

/// Deletes every `.rs` file under `output_dir` that `written` does not list — the leftovers of a renamed or deleted `.rsx`, an `auto_modules` directory that lost its last hand-written `.rs`, or a dropped i18n catalog. Only ever recurses through real subdirectories reached from `output_dir` itself (a symlink is skipped, not followed), so every path it can act on is provably inside the generated tree; it never removes a directory or a non-`.rs` file. Best-effort: a removal failure just leaves that orphan for next time.
pub fn prune_stale_generated(output_dir: &Path, written: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            prune_stale_generated(&path, written);
        } else if file_type.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("rs")
            && !written.contains(&path)
        {
            let _ = std::fs::remove_file(&path);
        }
    }
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

/// A `.rs` file that is a declarable module: not a crate root, not a `mod.rs` (the latter is the module root of its own directory, not a sibling child), and not a `*_test.rs`.
///
/// A `*_test.rs` holds the `#[cfg(test)]` body of the module it sits beside, which declares it itself with `#[path]`. Declaring it here too would make it a *sibling* of that module rather than a child, so its `use super::*` would reach the parent directory and every private item it tests would read as inaccessible.
fn is_rust_module_file(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    !matches!(name, "lib.rs" | "main.rs" | "mod.rs") && !name.ends_with("_test.rs")
}

/// Whether a directory earns a module node: it holds a declarable `.rs`, a `mod.rs`, or a `.rsx` — a directory of nothing but markup is still a module once its `.rsx` files live where they sit.
fn dir_has_rust_module(dir: &Path) -> bool {
    collect_files_by_ext(dir, &["rs"], &|_| true)
        .iter()
        .any(|p| is_rust_module_file(p) || p.file_name().and_then(|n| n.to_str()) == Some("mod.rs"))
        || !collect_files_by_ext(dir, &["rsx"], &|_| true).is_empty()
}

/// Whether the `mod.rs` invokes the macro that places its own `.rsx` siblings.
fn mod_rs_places_its_own(mod_rs: &Path) -> bool {
    std::fs::read_to_string(mod_rs)
        .map(|src| src.contains("rsx_modules!") || src.contains("app!"))
        .unwrap_or(false)
}

/// Whether any `.rsx` sits under `dir`, which is what makes a directory with no `mod.rs` worth declaring even when hand-written modules are the crate's own business.
fn dir_has_rsx(dir: &Path) -> bool {
    !collect_files_by_ext(dir, &["rsx"], &|_| true).is_empty()
}

/// Where the transpiler wrote a `.rsx`'s Rust. The generated tree mirrors the source tree, and `flat_prefix` is that same path with `__` between the segments, so the two agree by construction.
fn rsx_output(generated_dir: &Path, flat_prefix: &str, name: &str) -> PathBuf {
    let mut out = generated_dir.to_path_buf();
    for segment in flat_prefix.split("__").filter(|s| !s.is_empty()) {
        out.push(segment);
    }
    out.join(format!("{name}.rs"))
}

/// The name the component in `path` is generated under: its file stem.
///
/// A `.rsx` is a module where its file sits, so two files may share a basename — they are different modules, which is what a path-flattened name existed to work around. One function rather than each caller taking the stem itself: the editor and the golden harness both have to agree with the macro on this exactly, and they silently drifted apart when the flattening went.
pub fn component_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Derives the output `.rs` path (relative to the output root) for a `.rsx` file by mirroring its location under `src_dir`, so files in different directories never collide (e.g. `src/components/button.rsx` -> `components/button.rs`). Used for the transpiler's `.telar/build/` output. Returns `None` for files outside `src_dir`: those are never transpiled, so they have no place in the build tree — and flattening their absolute path would escape the output root entirely.
pub fn relative_output_path(path: &Path, src_dir: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(src_dir).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.with_extension("rs"))
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod tests;
