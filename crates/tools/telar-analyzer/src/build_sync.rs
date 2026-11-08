//! The transpiled view of the buffer being typed: computed on every successful parse, handed to the embedded analyzer in memory, and left on disk only where something other than the analyzer has to read it.
//!
//! What is live is the overlay ([`generated_target`] into `ra::vfs`), not the file. The generated `.rs` is written once, to give a newly created `.rsx` something the loaded workspace graph can carry, and never rewritten — `.telar/build/` is what a build compiles, and a second writer putting unsaved text there is how a build comes to compile something nobody saved. The map follows the buffer too, because reverse-mapping a position has to, but only in the analyzer's memory ([`crate::store::Store`]): the `.rs.map` beside the build file is the transpiler's alone, and is what `cargo telar check` reads.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use telar_baker::BakeReport;
use telar_diagnostics::{Severity, Span};
use telar_parser::{RsxDocument, ViewNode};
use telar_project::{
    AssetContext, AssetKind, BuildFlavour, TelarManifest, TelarSection, asset_kind_for_tag,
};
use telar_transpiler::SourceMap;

/// Nearest ancestor holding a `Cargo.toml` — the crate root, i.e. the macro's `CARGO_MANIFEST_DIR`. Anchored on `Cargo.toml` (not `telar.toml`) so this works in crates without an rsx config too.
pub fn crate_root(rsx_path: &Path) -> Option<PathBuf> {
    telar_project::find_package_root(rsx_path)
}

/// The transpiler output for one `.rsx`, computed in-memory (no disk). Shared by [`sync_build_file`] and the embedded-analyzer query paths so both see byte-identical generated text.
pub struct GeneratedTarget {
    /// Path of the generated `<crate>/.telar/build/<rel>.rs`.
    pub path: PathBuf,
    /// Generated Rust code.
    pub code: String,
    /// Where every part of [`Self::code`] came from — the lines, and the verbatim spans that make a column mean something. One value rather than two halves, because the two are only ever right together.
    pub map: SourceMap,
    /// The `[telar] prelude` [`Self::code`] was transpiled against, which an unknown tag's diagnostic lists.
    pub prelude: Vec<telar_project::PreludeEntry>,
}

pub enum NoTarget {
    /// It is outside a package's `src/`, its `telar.toml` does not load, or it does not parse; the buffer's diagnostics already say so where there is anything to say.
    Unavailable,
    /// The transpiler refused it, and the build will refuse it with this same message.
    Refused(String),
}

/// Transpiles `source` into a [`GeneratedTarget`] without touching disk. Shared by [`sync_build_file`] and the embedded-analyzer completion path so both see byte-identical generated text.
///
/// `None` when there is nothing to mirror, which [`target_or_refusal`] tells apart for the one caller that reports it.
pub fn generated_target(
    rsx_path: &Path,
    source: &str,
    theme_type: Option<&str>,
) -> Option<GeneratedTarget> {
    target_or_refusal(rsx_path, source, theme_type).ok()
}

pub fn target_or_refusal(
    rsx_path: &Path,
    source: &str,
    theme_type: Option<&str>,
) -> Result<GeneratedTarget, NoTarget> {
    let root = crate_root(rsx_path).ok_or(NoTarget::Unavailable)?;
    let telar = TelarManifest::load(&root)
        .map_err(|_| NoTarget::Unavailable)?
        .telar;
    transpile_target(&root, &telar, rsx_path, source, theme_type)
}

fn transpile_target(
    root: &Path,
    telar: &TelarSection,
    rsx_path: &Path,
    source: &str,
    theme_type: Option<&str>,
) -> Result<GeneratedTarget, NoTarget> {
    let src_dir = root.join("src");
    let rel =
        telar_project::relative_output_path(rsx_path, &src_dir).ok_or(NoTarget::Unavailable)?;
    // The same artifact the macro reads, so the mirror shows the same `src:"…"` errors the build will. Loaded, never baked: this runs on the completion and hover paths too, and neither should be spawning cargo.
    let assets = AssetContext::load(root, telar, &project_telar_version(root));
    let prelude = telar.prelude().to_vec();
    // No cross-file pre-pass: the editor mirrors the build exactly, because neither needs to know what any other file declares. A component call spells names, and the callee's own type answers for them.
    let result = telar_transpiler::transpile_buffer(
        rsx_path,
        source,
        &telar_transpiler::PackageOptions {
            src_dir: &src_dir,
            theme_type,
            assets: Some(&assets),
            prelude: &prelude,
            library: telar.library,
            flavour: BuildFlavour::Plain,
        },
    )
    .map_err(|error| match error {
        telar_transpiler::TranspileError::Codegen(message) => NoTarget::Refused(message),
        telar_transpiler::TranspileError::Parse(_) => NoTarget::Unavailable,
    })?;
    let out_path = telar_project::generated_dir(root, BuildFlavour::Plain).join(&rel);
    Ok(GeneratedTarget {
        path: out_path,
        code: result.rust_code,
        map: {
            let mut map = SourceMap::new(result.source_map, result.expr_spans);
            map.shadows = result.shadows;
            map
        },
        prelude,
    })
}

/// The `telar` version this project resolves — what a baked artifact must be written against, and what the macro will compare it to. Not this binary's own: `telar-analyzer` is installed once per machine and answers for projects pinning any version, so `env!("CARGO_PKG_VERSION")` here would report a mismatch the project's own build never sees.
///
/// Cached against the lockfile's mtime, because resolving it shells out to `cargo metadata` and this is reached on every keystroke. A dependency edit changes the lockfile and re-resolves; nothing else does.
fn project_telar_version(package_dir: &Path) -> String {
    type VersionCache = Mutex<HashMap<PathBuf, (Option<SystemTime>, String)>>;
    static CACHE: OnceLock<VersionCache> = OnceLock::new();

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

type Bake = dyn Fn(&Path, &str) -> Option<BakeReport> + Send + Sync;

struct FailedBake {
    content: u64,
    report: Arc<BakeReport>,
}

struct AssetReference {
    kind: &'static AssetKind,
    path: String,
    line: usize,
}

/// Bakes a package's assets for the editor when the artifact cannot answer for something a buffer references — it was never baked, it is stale, or a `src:"…"` was just typed for a file no build has seen. Without this the editor shows a `compile_error!` telling the user to run a command, on a keystroke, in a buffer they have not saved.
///
/// A bake that reported errors is remembered by the asset's path and a hash of the bytes (and of the `telar.toml`) it failed on, so later keystrokes replay its report instead of re-parsing the whole package, until one of those changes.
pub struct Bakes {
    bake: Box<Bake>,
    failed: Mutex<HashMap<PathBuf, FailedBake>>,
}

impl Bakes {
    pub fn new(bake: impl Fn(&Path, &str) -> Option<BakeReport> + Send + Sync + 'static) -> Self {
        Self {
            bake: Box::new(bake),
            failed: Mutex::default(),
        }
    }

    pub fn editor() -> Self {
        let producer = format!("telar-analyzer {}", env!("CARGO_PKG_VERSION"));
        Self::new(move |package_dir, version| {
            telar_baker::bake_package(package_dir, &producer, version)
        })
    }

    /// Bakes only when a file the buffer names is actually on disk. A `src:` pointing at nothing — a typo, or a path being typed one character at a time — is unresolvable no matter how often it is baked, and this runs on every keystroke. Returns what the bake reported, as diagnostics on the lines of `document` that caused it.
    pub fn bake_if_unanswered(
        &self,
        rsx_path: &Path,
        source: &str,
        document: &RsxDocument,
    ) -> Vec<telar_diagnostics::Diagnostic> {
        let Some(package_dir) = crate_root(rsx_path) else {
            return Vec::new();
        };
        let Ok(manifest) = TelarManifest::load(&package_dir) else {
            return Vec::new();
        };
        let telar = manifest.telar;
        let version = project_telar_version(&package_dir);
        let assets = AssetContext::load(&package_dir, &telar, &version);
        let assets_root = telar.assets_root(&package_dir);
        let references = asset_references(document, source);
        let bakeable: Vec<PathBuf> = references
            .iter()
            .filter(|reference| assets.resolve(reference.kind, &reference.path).is_err())
            .map(|reference| assets_root.join(&reference.path))
            .filter(|file| file.is_file())
            .collect();
        if bakeable.is_empty() {
            return Vec::new();
        }
        let manifest_bytes = std::fs::read(package_dir.join(telar_project::MANIFEST_FILENAME)).ok();
        let hashed: Vec<(PathBuf, u64)> = bakeable
            .into_iter()
            .map(|file| {
                let content = content_hash(&file, manifest_bytes.as_deref());
                (file, content)
            })
            .collect();

        let reports = {
            let mut failed = self.failed.lock().unwrap_or_else(|e| e.into_inner());
            let remembered: Option<Vec<Arc<BakeReport>>> = hashed
                .iter()
                .map(|(file, content)| {
                    failed
                        .get(file)
                        .filter(|known| known.content == *content)
                        .map(|known| known.report.clone())
                })
                .collect();
            match remembered {
                Some(reports) => reports,
                None => {
                    let Some(report) = (self.bake)(&package_dir, &version) else {
                        return Vec::new();
                    };
                    let report = Arc::new(report);
                    for (file, content) in hashed {
                        if report.errors.is_empty() {
                            failed.remove(&file);
                        } else {
                            failed.insert(
                                file,
                                FailedBake {
                                    content,
                                    report: report.clone(),
                                },
                            );
                        }
                    }
                    vec![report]
                }
            }
        };

        let mut distinct: Vec<Arc<BakeReport>> = Vec::new();
        for report in reports {
            if !distinct.iter().any(|seen| Arc::ptr_eq(seen, &report)) {
                distinct.push(report);
            }
        }
        distinct
            .iter()
            .flat_map(|report| report_diagnostics(report, &package_dir, rsx_path, &references))
            .collect()
    }
}

fn content_hash(asset: &Path, manifest: Option<&[u8]>) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::fs::read(asset).ok().hash(&mut hasher);
    manifest.hash(&mut hasher);
    hasher.finish()
}

fn asset_references(document: &RsxDocument, source: &str) -> Vec<AssetReference> {
    let mut out = Vec::new();
    collect_references(&document.view.nodes, source, &mut out);
    for preview in &document.previews {
        collect_references(&preview.body, source, &mut out);
    }
    out
}

fn collect_references(nodes: &[ViewNode], source: &str, out: &mut Vec<AssetReference>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                if let Some(kind) = asset_kind_for_tag(&el.tag) {
                    for attr in &el.attributes {
                        let path = attr.value.text().trim();
                        if attr.key == kind.attr && attr.value.is_quoted() && !path.is_empty() {
                            let line = source
                                .get(..attr.value_start)
                                .map_or(el.line, |before| before.matches('\n').count() + 1);
                            out.push(AssetReference {
                                kind,
                                path: path.to_string(),
                                line,
                            });
                        }
                    }
                }
                collect_references(&el.children, source, out);
            }
            ViewNode::IfBlock(block) => {
                collect_references(&block.then_branch, source, out);
                if let Some(else_branch) = &block.else_branch {
                    collect_references(else_branch, source, out);
                }
            }
            ViewNode::ForBlock(block) => collect_references(&block.body, source, out),
            ViewNode::MatchBlock(block) => {
                for arm in &block.arms {
                    collect_references(&arm.body, source, out);
                }
            }
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => {}
        }
    }
}

fn report_diagnostics(
    report: &BakeReport,
    package_dir: &Path,
    rsx_path: &Path,
    references: &[AssetReference],
) -> Vec<telar_diagnostics::Diagnostic> {
    let own_file = rsx_path
        .strip_prefix(package_dir)
        .unwrap_or(rsx_path)
        .display()
        .to_string();
    let errors = report
        .errors
        .iter()
        .map(|message| (Severity::Error, message));
    let warnings = report
        .warnings
        .iter()
        .map(|message| (Severity::Warning, message));
    let mut out = Vec::new();
    for (severity, message) in errors.chain(warnings) {
        let lines: Vec<usize> = match located_at(message) {
            Some((file, line)) if file == own_file => vec![line],
            Some(_) => Vec::new(),
            None => {
                let named: Vec<usize> = references
                    .iter()
                    .filter(|reference| message.contains(&format!("`{}`", reference.path)))
                    .map(|reference| reference.line)
                    .collect();
                if named.is_empty() { vec![1] } else { named }
            }
        };
        out.extend(lines.into_iter().map(|line| {
            telar_diagnostics::Diagnostic::new(severity, message.clone(), Span::line(line))
        }));
    }
    out
}

fn located_at(message: &str) -> Option<(&str, usize)> {
    let (location, _) = message.split_once(": ")?;
    let (file, line) = location.rsplit_once(':')?;
    Some((file, line.parse().ok()?))
}

/// Leaves the generated file if nothing has produced one yet, and returns the build path with the buffer's map. `theme_type` matches the analyzer's project discovery. A no-op when the file is outside a crate's `src/`, its `telar.toml` does not load, or transpilation fails, so IntelliSense keeps working against the last parseable state.
pub fn sync_build_file(
    rsx_path: &Path,
    source: &str,
    theme_type: Option<&str>,
) -> Option<(PathBuf, SourceMap)> {
    let root = crate_root(rsx_path)?;
    let telar = TelarManifest::load(&root).ok()?.telar;
    let GeneratedTarget {
        path, code, map, ..
    } = transpile_target(&root, &telar, rsx_path, source, theme_type).ok()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Written to give the file something to exist as, and never rewritten. The embedded analyzer only knows a file the loaded workspace graph carries, and a `.rsx` created in the editor has no generated Rust until something builds it; every edit after that is served by the in-memory overlay, so overwriting here would buy nothing and cost the one thing this directory has to guarantee — that what a build compiles came from what is on disk, and not from a buffer nobody has saved.
    if !path.exists() {
        let _ = telar_project::write_if_changed_atomic(&path, &code);
    }
    Some((path, map))
}

/// Whether `path` is generated output, which the backend has to know before it reverse-maps a definition target onto a `.rsx`.
pub use telar_project::is_generated_output as is_generated_build_file;

/// The `.rsx` a generated file came from, plus its source map: `live`, the map of a buffer the editor has open, else the transpiler's beside the build file. `None` outside a build directory, or without a readable map.
pub fn rsx_source_and_map(
    build_path: &Path,
    live: Option<&SourceMap>,
) -> Option<(PathBuf, SourceMap)> {
    let source = telar_project::source_for_generated(build_path)?;
    let map = match live {
        Some(live) => live.clone(),
        None => SourceMap::from_json(
            &std::fs::read_to_string(build_path.with_extension("rs.map")).ok()?,
        )?,
    };
    Some((source, map))
}

#[cfg(test)]
#[path = "build_sync.test.rs"]
mod tests;
