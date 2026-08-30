//! Live mirror of the `app!` macro's compile-time output. On every successful parse of a `.rsx`, the transpiled Rust is written to `<crate>/.telar/build/<rel>.rs` (and its `.rs.map`) so the workspace rust-analyzer analyzes the in-flight buffer — making completion / hover / definition live instead of one `cargo check` behind, since rust-analyzer re-reads the changed file from disk.

use std::path::{Path, PathBuf};

use telar_transpiler::SourceMap;
use telar_transpiler::find_ancestor_dir;

/// Nearest ancestor holding a `Cargo.toml` — the crate root, i.e. the macro's `CARGO_MANIFEST_DIR`. Anchored on `Cargo.toml` (not `telar.toml`) so this works in crates without an rsx config too.
pub fn crate_root(rsx_path: &Path) -> Option<PathBuf> {
    find_ancestor_dir(rsx_path, |dir| dir.join("Cargo.toml").exists())
}

/// The generated `.rs` path for an `.rsx`, without transpiling (the content is written separately by [`sync_build_file`]). Used to warm the analyzer for a file the moment it opens.
pub fn generated_path(rsx_path: &Path) -> Option<PathBuf> {
    let root = crate_root(rsx_path)?;
    let src_dir = root.join("src");
    let rel = telar_transpiler::relative_output_path(rsx_path, &src_dir)?;
    Some(root.join(".telar").join("build").join(rel))
}

/// The transpiler output for one `.rsx`, computed in-memory (no disk). Shared by [`sync_build_file`] and the embedded-analyzer query paths so both see byte-identical generated text.
pub struct GeneratedTarget {
    /// Path of the generated `<crate>/.telar/build/<rel>.rs`.
    pub path: PathBuf,
    /// Generated Rust code.
    pub code: String,
    /// Where every part of [`Self::code`] came from — the lines, and the verbatim spans that make a column
    /// mean something. One value rather than two halves, because the two are only ever right together.
    pub map: SourceMap,
}

/// Transpiles `source` into a [`GeneratedTarget`] without touching disk. Shared by [`sync_build_file`] and the embedded-analyzer completion path so both see byte-identical generated text.
pub fn generated_target(
    rsx_path: &Path,
    source: &str,
    theme_type: Option<&str>,
) -> Option<GeneratedTarget> {
    let root = crate_root(rsx_path)?;
    let src_dir = root.join("src");
    let rel = telar_transpiler::relative_output_path(rsx_path, &src_dir)?;
    let stem = telar_transpiler::relative_stem(rsx_path, &src_dir);
    // Match the macro: baked `src:"..."` paths resolve against the project asset root, not the `.rsx` dir.
    let assets_root = telar_transpiler::assets_root(&root);
    // No cross-file pre-pass: the editor mirrors the build exactly because neither one needs to know what
    // any other file declares. A component call spells names, and the callee's own type answers for them.
    let result =
        telar_transpiler::transpile_source(source, &stem, theme_type, Some(assets_root.as_path()))
            .ok()?;
    let out_path = root.join(".telar").join("build").join(&rel);
    Some(GeneratedTarget {
        path: out_path,
        code: result.rust_code,
        map: SourceMap::new(result.source_map, result.expr_spans),
    })
}

/// Transpiles `source` and writes `<crate>/.telar/build/<rel>.rs` + `.rs.map`, mirroring the macro's output so rust-analyzer sees the live buffer. `theme_type` matches the analyzer's project discovery. A no-op when the file is outside a crate's `src/` or transpilation fails — the last good build stays, so IntelliSense keeps working against the last parseable state.
pub fn sync_build_file(rsx_path: &Path, source: &str, theme_type: Option<&str>) {
    let Some(GeneratedTarget { path, code, map }) = generated_target(rsx_path, source, theme_type)
    else {
        return;
    };
    write_if_changed(&path, &code);
    write_if_changed(&path.with_extension("rs.map"), &map.to_json());
}

/// The `<crate>/.telar/build/` path segment that marks a generated file (platform separators).
/// Splits `<crate>/.telar/build/<rel>.rs` into (`<crate>`, `<rel>.rs`). Component-based instead of string matching so Windows paths with mixed `/`/`\` separators still classify.
fn split_at_build_dir(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let comps: Vec<std::path::Component> = path.components().collect();
    let pos = comps
        .windows(2)
        .position(|w| w[0].as_os_str() == ".telar" && w[1].as_os_str() == "build")?;
    let rel: PathBuf = comps[pos + 2..].iter().collect();
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some((comps[..pos].iter().collect(), rel))
}

/// Whether `path` is one of the transpiler's generated build files (`<crate>/.telar/build/<rel>.rs`). Used to classify a rust-analyzer definition target before reverse-mapping it onto a `.rsx`.
pub fn is_generated_build_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs") && split_at_build_dir(path).is_some()
}

/// Inverse of [`generated_target`]'s path mapping: a generated `<crate>/.telar/build/<rel>.rs` → its source `<crate>/src/<rel>.rsx` plus the source map read from the sibling `.rs.map`. `None` for paths outside a build dir or without a readable map.
pub fn rsx_source_and_map(build_path: &Path) -> Option<(PathBuf, SourceMap)> {
    let source = rsx_source_for(build_path)?;
    let map_json = std::fs::read_to_string(build_path.with_extension("rs.map")).ok()?;
    Some((source, SourceMap::from_json(&map_json)?))
}

/// `<crate>/.telar/build/<rel>.rs` → `<crate>/src/<rel>.rsx` (the macro's output mirroring, reversed).
fn rsx_source_for(build_path: &Path) -> Option<PathBuf> {
    let (root, rel) = split_at_build_dir(build_path)?;
    let rel = rel.to_str()?.strip_suffix(".rs")?.to_string();
    Some(root.join("src").join(format!("{rel}.rsx")))
}

/// Writes only when the content differs, so rust-analyzer's file watcher doesn't churn on no-op edits.
fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path)
        .map(|existing| existing == content)
        .unwrap_or(false)
    {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, content);
}
