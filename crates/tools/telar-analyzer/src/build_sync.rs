//! The transpiled view of the buffer being typed: computed on every successful parse, handed to the embedded analyzer in memory, and left on disk only where something other than the analyzer has to read it.
//!
//! What is live is the overlay ([`generated_target`] into `ra::vfs`), not the file. The generated `.rs` is written once, to give a newly created `.rsx` something the loaded workspace graph can carry, and never rewritten — `.telar/build/` is what a build compiles, and a second writer putting unsaved text there is how a build comes to compile something nobody saved. The `.rs.map` beside it is rewritten every time, because reverse-mapping a position has to follow the buffer and no build reads it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use telar_parser::RsxDocument;
use telar_project::{AssetContext, BuildFlavour, find_ancestor_dir};
use telar_transpiler::SourceMap;

/// Nearest ancestor holding a `Cargo.toml` — the crate root, i.e. the macro's `CARGO_MANIFEST_DIR`. Anchored on `Cargo.toml` (not `telar.toml`) so this works in crates without an rsx config too.
pub fn crate_root(rsx_path: &Path) -> Option<PathBuf> {
    find_ancestor_dir(rsx_path, |dir| dir.join("Cargo.toml").exists())
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
    let rel = telar_project::relative_output_path(rsx_path, &src_dir)?;
    let stem = telar_project::component_name(rsx_path);
    // The same artifact the macro reads, so the mirror shows the same `src:"…"` errors the build will. Loaded, never baked: this runs on the completion and hover paths too, and neither should be spawning cargo.
    let assets = AssetContext::load(&root, &project_telar_version(&root));
    // No cross-file pre-pass: the editor mirrors the build exactly, because neither needs to know what any other file declares. A component call spells names, and the callee's own type answers for them.
    let result = match telar_project::is_module_root(rsx_path) {
        true => {
            telar_transpiler::transpile_module_root(source, telar_project::MODULE_CHILDREN_FILENAME)
        }
        false => telar_transpiler::transpile_source(source, &stem, theme_type, Some(&assets)),
    }
    .ok()?;
    let out_path = telar_project::generated_dir(&root, BuildFlavour::Plain).join(&rel);
    Some(GeneratedTarget {
        path: out_path,
        code: result.rust_code,
        map: {
            let mut map = SourceMap::new(result.source_map, result.expr_spans);
            map.shadows = result.shadows;
            map
        },
    })
}

/// The `telar` version this project resolves — what a baked artifact must be written against, and what the macro will compare it to. Not this binary's own: `telar-analyzer` is installed once per machine and answers for projects pinning any version, so `env!("CARGO_PKG_VERSION")` here would report a mismatch the project's own build never sees.
///
/// Cached against the lockfile's mtime, because resolving it shells out to `cargo metadata` and this is reached on every keystroke. A dependency edit changes the lockfile and re-resolves; nothing else does.
fn project_telar_version(package_dir: &Path) -> String {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Option<SystemTime>, String)>>> = OnceLock::new();

    let workspace_root = telar_project::find_workspace_root(package_dir)
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
    let version = telar_project::resolve_telar_version(&workspace_root)
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
    let assets_root = telar_project::assets_root(package_dir);
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

/// Bakes what the buffer references if the artifact cannot answer for it, then leaves the map beside the generated file — and the file itself if nothing has produced one yet. `theme_type` matches the analyzer's project discovery. A no-op when the file is outside a crate's `src/` or transpilation fails, so IntelliSense keeps working against the last parseable state.
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
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Written to give the file something to exist as, and never rewritten. The embedded analyzer only knows a file the loaded workspace graph carries, and a `.rsx` created in the editor has no generated Rust until something builds it; every edit after that is served by the in-memory overlay, so overwriting here would buy nothing and cost the one thing this directory has to guarantee — that what a build compiles came from what is on disk, and not from a buffer nobody has saved.
    if !path.exists() {
        let _ = telar_project::write_if_changed_atomic(&path, &code);
    }
    // The map does track the live buffer: it is what turns a position in the overlaid Rust back into one in the `.rsx` being typed, and it is not what the build artifact attests to.
    let _ = telar_project::write_if_changed_atomic(&path.with_extension("rs.map"), &map.to_json());
}

/// Whether `path` is generated output, which the backend has to know before it reverse-maps a definition target onto a `.rsx`.
pub use telar_project::is_generated_output as is_generated_build_file;

/// The `.rsx` a generated file came from, plus the source map read from its sibling `.rs.map`. `None` outside a build directory, or without a readable map.
pub fn rsx_source_and_map(build_path: &Path) -> Option<(PathBuf, SourceMap)> {
    let source = telar_project::source_for_generated(build_path)?;
    let map_json = std::fs::read_to_string(build_path.with_extension("rs.map")).ok()?;
    Some((source, SourceMap::from_json(&map_json)?))
}

#[cfg(test)]
#[path = "build_sync_test.rs"]
mod tests;
