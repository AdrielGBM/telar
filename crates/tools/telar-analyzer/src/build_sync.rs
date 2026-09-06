//! Live mirror of the `app!` macro's compile-time output. On every successful parse of a `.rsx`, the transpiled Rust is written to `<crate>/.telar/build/<rel>.rs` (and its `.rs.map`) so the workspace rust-analyzer analyzes the in-flight buffer — making completion / hover / definition live instead of one `cargo check` behind, since rust-analyzer re-reads the changed file from disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use telar_parser::RsxDocument;
use telar_transpiler::{AssetContext, SourceMap, find_ancestor_dir};

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
    /// Where every part of [`Self::code`] came from — the lines, and the verbatim spans that make a column mean something. One value rather than two halves, because the two are only ever right together.
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
    let stem = telar_transpiler::component_name(rsx_path);
    // The same artifact the macro reads, so the mirror shows the same `src:"…"` errors the build will. Loaded, never baked: this runs on the completion and hover paths too, and neither should be spawning cargo.
    let assets = AssetContext::load(&root, &project_telar_version(&root));
    // No cross-file pre-pass: the editor mirrors the build exactly, because neither needs to know what any other file declares. A component call spells names, and the callee's own type answers for them.
    let result =
        telar_transpiler::transpile_source(source, &stem, theme_type, Some(&assets)).ok()?;
    let out_path = root.join(".telar").join("build").join(&rel);
    Some(GeneratedTarget {
        path: out_path,
        code: result.rust_code,
        map: SourceMap::new(result.source_map, result.expr_spans),
    })
}

/// The `telar` version this project resolves — what a baked artifact must be written against, and what the macro will compare it to. Not this binary's own: `telar-analyzer` is installed once per machine and answers for projects pinning any version, so `env!("CARGO_PKG_VERSION")` here would report a mismatch the project's own build never sees.
///
/// Cached against the lockfile's mtime, because resolving it shells out to `cargo metadata` and this is reached on every keystroke. A dependency edit changes the lockfile and re-resolves; nothing else does.
fn project_telar_version(package_dir: &Path) -> String {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Option<SystemTime>, String)>>> = OnceLock::new();

    let workspace_root = telar_transpiler::find_workspace_root(package_dir)
        .unwrap_or_else(|| package_dir.to_path_buf());
    let stamp = std::fs::metadata(workspace_root.join("Cargo.lock"))
        .and_then(|meta| meta.modified())
        .ok();

    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_stamp, version)) = cache.get(&workspace_root)
        && *cached_stamp == stamp
    {
        return version.clone();
    }
    // Falling back to this binary's version keeps the mirror working when cargo cannot run at all; it is wrong for a project pinning another `telar`, but so is every other answer available here.
    let version = telar_baker::resolve_telar_version(&workspace_root)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    cache.insert(workspace_root, (stamp, version.clone()));
    version
}

/// Bakes `package_dir`'s assets when the artifact cannot answer for something `document` references — it was never baked, it is stale, or a `src:"…"` was just typed for a file no build has seen. Without this the editor shows a `compile_error!` telling the user to run a command, on a keystroke, in a buffer they have not saved.
///
/// Only when the file it names is actually on disk. A `src:` pointing at nothing — a typo, or a path being typed one character at a time — is unresolvable no matter how often it is baked, and this runs on every keystroke: without that guard a mistyped path re-parses every `.rsx` in the package until it is fixed.
fn bake_if_unanswered(package_dir: &Path, document: &RsxDocument) {
    let version = project_telar_version(package_dir);
    let assets = AssetContext::load(package_dir, &version);
    let assets_root = telar_transpiler::assets_root(package_dir);
    let mut refs = Vec::new();
    telar_baker::collect_asset_refs(document, &mut refs);
    let bakeable = refs.iter().any(|(kind, path)| {
        assets.resolve(kind, path).is_err() && assets_root.join(path).is_file()
    });
    if !bakeable {
        return;
    }
    let producer = format!("telar-analyzer {}", env!("CARGO_PKG_VERSION"));
    telar_baker::bake_package(package_dir, &producer, &version);
}

/// Transpiles `source` and writes `<crate>/.telar/build/<rel>.rs` + `.rs.map`, mirroring the macro's output so rust-analyzer sees the live buffer. `theme_type` matches the analyzer's project discovery. A no-op when the file is outside a crate's `src/` or transpilation fails — the last good build stays, so IntelliSense keeps working against the last parseable state.
pub fn sync_build_file(
    rsx_path: &Path,
    source: &str,
    document: &RsxDocument,
    theme_type: Option<&str>,
) {
    if let Some(root) = crate_root(rsx_path) {
        bake_if_unanswered(&root, document);
    }
    let Some(GeneratedTarget { path, code, map }) = generated_target(rsx_path, source, theme_type)
    else {
        return;
    };
    write_if_changed(&path, &code);
    write_if_changed(&path.with_extension("rs.map"), &map.to_json());
}

/// The `<crate>/.telar/build/` path segment that marks a generated file (platform separators). Splits `<crate>/.telar/build/<rel>.rs` into (`<crate>`, `<rel>.rs`). Component-based instead of string matching so Windows paths with mixed `/`/`\` separators still classify.
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

#[cfg(test)]
mod tests {
    use super::*;

    const ICON_SVG: &[u8] = include_bytes!("../../telar-baker/tests/fixtures/icon.svg");

    /// A crate laid out the way an editor finds one, with an asset referenced but never baked.
    fn unbaked_crate(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("telar_analyzer_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
        std::fs::write(root.join("assets/badge.svg"), ICON_SVG).unwrap();
        root
    }

    /// The whole point of T-6.1: a `src:"…"` the editor sees before any build has must not become a `compile_error!` telling the user to go and run a command.
    #[test]
    fn a_never_baked_asset_is_baked_rather_than_mirrored_as_an_error() {
        let root = unbaked_crate("never_baked");
        let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
        let rsx = root.join("src/view.rsx");
        std::fs::write(&rsx, source).unwrap();
        let document = telar_parser::parse(source).unwrap();

        sync_build_file(&rsx, source, &document, None);

        assert!(
            root.join(".telar/assets.json").is_file(),
            "the editor should have baked the artifact it found missing"
        );
        let mirrored = std::fs::read_to_string(root.join(".telar/build/view.rs")).unwrap();
        assert!(
            mirrored.contains("crate::__rsx_assets::ASSET_"),
            "the mirror should reference the baked static:\n{mirrored}"
        );
        assert!(
            !mirrored.contains("compile_error!"),
            "the mirror should carry no asset error:\n{mirrored}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A path naming nothing is the common editor state — it is what a half-typed one looks like — and no amount of baking resolves it, so it must not trigger one on every keystroke.
    #[test]
    fn a_src_naming_no_file_bakes_nothing() {
        let root = unbaked_crate("no_such_file");
        let source = "[view]\ncol\n    svg src:\"nope.svg\"\n";
        let rsx = root.join("src/view.rsx");
        std::fs::write(&rsx, source).unwrap();
        let document = telar_parser::parse(source).unwrap();

        sync_build_file(&rsx, source, &document, None);

        assert!(
            !root.join(".telar/assets.json").exists(),
            "a missing file is not something a bake can supply"
        );
        let mirrored = std::fs::read_to_string(root.join(".telar/build/view.rs")).unwrap();
        assert!(mirrored.contains("compile_error!"), "{mirrored}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The counterpart: once the file exists, the very next sync must pick it up. Suppressing the bake for a missing file must not also suppress it for one that has just appeared.
    #[test]
    fn an_asset_created_after_a_failed_sync_is_picked_up() {
        let root = unbaked_crate("appears_later");
        let source = "[view]\ncol\n    svg src:\"later.svg\"\n";
        let rsx = root.join("src/view.rsx");
        std::fs::write(&rsx, source).unwrap();
        let document = telar_parser::parse(source).unwrap();
        sync_build_file(&rsx, source, &document, None);
        assert!(!root.join(".telar/assets.json").exists());

        std::fs::write(root.join("assets/later.svg"), ICON_SVG).unwrap();
        sync_build_file(&rsx, source, &document, None);

        let mirrored = std::fs::read_to_string(root.join(".telar/build/view.rs")).unwrap();
        assert!(
            mirrored.contains("crate::__rsx_assets::ASSET_"),
            "{mirrored}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// An edit to the asset itself, which leaves the index listing it at an old hash.
    #[test]
    fn an_asset_edited_since_the_bake_is_rebaked() {
        let root = unbaked_crate("stale");
        let source = "[view]\ncol\n    svg src:\"badge.svg\"\n";
        let rsx = root.join("src/view.rsx");
        std::fs::write(&rsx, source).unwrap();
        let document = telar_parser::parse(source).unwrap();
        sync_build_file(&rsx, source, &document, None);
        let first = std::fs::read_to_string(root.join(".telar/assets.json")).unwrap();

        std::fs::write(root.join("assets/badge.svg"), b"<svg viewBox=\"0 0 9 9\"/>").unwrap();
        sync_build_file(&rsx, source, &document, None);

        let second = std::fs::read_to_string(root.join(".telar/assets.json")).unwrap();
        assert_ne!(first, second, "the hash should have been rewritten");
        let mirrored = std::fs::read_to_string(root.join(".telar/build/view.rs")).unwrap();
        assert!(!mirrored.contains("compile_error!"), "{mirrored}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
