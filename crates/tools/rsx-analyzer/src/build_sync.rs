//! Live mirror of the `app!` macro's compile-time output. On every successful parse of a `.rsx`, the
//! transpiled Rust is written to `<crate>/.rsx/build/<rel>.rs` (and its `.rs.map`) so the workspace
//! rust-analyzer analyzes the in-flight buffer — making completion / hover / definition live instead
//! of one `cargo check` behind, since rust-analyzer re-reads the changed file from disk.

use std::path::{Path, PathBuf};

use rsx_workspace::find_ancestor_dir;

/// Nearest ancestor holding a `Cargo.toml` — the crate root, i.e. the macro's `CARGO_MANIFEST_DIR`.
/// Anchored on `Cargo.toml` (not `rsx.toml`) so this works in crates without an rsx config too.
fn crate_root(rsx_path: &Path) -> Option<PathBuf> {
    find_ancestor_dir(rsx_path, |dir| dir.join("Cargo.toml").exists())
}

/// Transpiles `source` and writes `<crate>/.rsx/build/<rel>.rs` + `.rs.map`, mirroring the macro's
/// output so rust-analyzer sees the live buffer. `theme_type` matches the analyzer's project discovery.
/// A no-op when the file is outside a crate's `src/` or transpilation fails — the last good build stays,
/// so IntelliSense keeps working against the last parseable state.
pub fn sync_build_file(rsx_path: &Path, source: &str, theme_type: Option<&str>) {
    let Some(root) = crate_root(rsx_path) else {
        return;
    };
    let src_dir = root.join("src");
    let Some(rel) = rsx_transpiler::relative_output_path(rsx_path, &src_dir) else {
        return;
    };
    let stem = rsx_transpiler::relative_stem(rsx_path, &src_dir);
    let Ok(result) = rsx_transpiler::transpile_source_with_theme(source, &stem, theme_type) else {
        return;
    };

    let out_path = root.join(".rsx").join("build").join(&rel);
    write_if_changed(&out_path, &result.rust_code);
    write_if_changed(
        &out_path.with_extension("rs.map"),
        &rsx_transpiler::source_map_to_json(&result.source_map),
    );
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
