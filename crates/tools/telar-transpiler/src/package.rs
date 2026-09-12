//! Transpiling a whole package: the walk over `src/**/*.rsx`, and where the generated Rust lands.
//!
//! The loop itself is what this exists for. It was written three times — once inside the `app!` macro, once in the golden harness, and a third time in the editor's live mirror — and the copies drifted: which name a file is transpiled under, which theme it resolves, whether the hot-reload rewrite applies. All three now ask this module, so a disagreement is a compile error rather than a snapshot nobody re-reads.
//!
//! Writing is separate from transpiling ([`write_package`] from [`transpile_package`]) because the golden harness compares output it must never put on disk, and the macro needs the set of paths it wrote to tell live output from what a deleted `.rsx` left behind.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use telar_project::{BUILD_ARTIFACT_FORMAT, BuildEntry, BuildFlavour, BuildIndex, relative_source};

use crate::codegen::{TranspileInput, TranspiledSource, transpile};
use crate::error::TranspileError;
use crate::source_map::SourceMap;
use telar_project::AssetContext;

/// Everything a package transpile needs to know, and nothing it can read behind the caller's back.
#[cfg(feature = "transpile")]
pub struct PackageOptions<'a> {
    /// The package's `src/`, which every `.rsx` under it is transpiled and every output path is mirrored from.
    pub src_dir: &'a Path,
    /// Concrete theme type path the package's `use_theme` resolves against — see [`crate::resolve_theme_type`], which is how a caller that did not parse the `app!` invocation itself gets one.
    pub theme_type: Option<&'a str>,
    /// The package's baked asset artifact, which static `svg`/`img` `src:"…"` references resolve against.
    pub assets: Option<&'a AssetContext>,
    /// Which shape to produce. Carries whether the build is hot-reloadable *and* whether it emits `[preview]` fns, because both change the Rust for the same source and each pair needs its own output directory.
    pub flavour: BuildFlavour,
}

/// One transpiled `.rsx`: where it came from, where it goes, and what it produced.
#[cfg(feature = "transpile")]
#[derive(Debug)]
pub struct GeneratedFile {
    pub rsx_path: PathBuf,
    /// Content hash of the source this was produced from — recorded here rather than re-read later, so the artifact cannot claim an output belongs to a file it never saw.
    pub source_hash: String,
    /// Output path relative to the generated directory, mirroring the file's place under `src/`.
    pub rel_out: PathBuf,
    pub source: TranspiledSource,
}

#[cfg(feature = "transpile")]
impl GeneratedFile {
    /// The `.rs` this file is written to under `generated_dir`.
    pub fn out_path(&self, generated_dir: &Path) -> PathBuf {
        generated_dir.join(&self.rel_out)
    }
}

#[cfg(feature = "transpile")]
/// What stopped a package transpile, with the file it happened in — the whole reason this is not a bare [`TranspileError`], which knows a line but not which file's.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// Formatted as the compiler formats a location, so a macro relaying this as `compile_error!` already names the `.rsx` and the line rather than the generated Rust.
    #[error("{}:{line}: {message}", path.display())]
    Parse {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("failed to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to transpile {}: {source}", path.display())]
    Codegen {
        path: PathBuf,
        source: TranspileError,
    },
    #[error("failed to write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[cfg(feature = "transpile")]
/// Transpiles every `.rsx` under `options.src_dir`, in a stable order, touching no disk beyond reading the sources.
///
/// Each file is transpiled under its own stem and knows nothing of its siblings, so this is a plain map over the walk: there is no cross-file pre-pass to keep in step, and adding one would be what makes a single-file edit re-read the whole package.
pub fn transpile_package(options: &PackageOptions<'_>) -> Result<Vec<GeneratedFile>, PackageError> {
    telar_project::find_rsx_files(options.src_dir)
        .into_iter()
        .filter_map(|rsx_path| {
            // Outside `src/` there is no place in the mirrored output tree, and `find_rsx_files` yields nothing outside it — so this drops nothing in practice and refuses to invent a path if it ever does.
            let rel_out = telar_project::relative_output_path(&rsx_path, options.src_dir)?;
            Some(transpile_one(rsx_path, rel_out, options))
        })
        .collect()
}

#[cfg(feature = "transpile")]
fn transpile_one(
    rsx_path: PathBuf,
    rel_out: PathBuf,
    options: &PackageOptions<'_>,
) -> Result<GeneratedFile, PackageError> {
    let source = std::fs::read_to_string(&rsx_path).map_err(|source| PackageError::Read {
        path: rsx_path.clone(),
        source,
    })?;
    let document = telar_parser::parse(&source).map_err(|e| PackageError::Parse {
        path: rsx_path.clone(),
        line: e.line,
        message: e.message.clone(),
    })?;
    let source_hash = telar_project::content_hash(source.as_bytes());
    let component_name = telar_project::component_name(&rsx_path);
    let generated = match telar_project::is_module_root(&rsx_path) {
        true => crate::codegen::module_root(&document, telar_project::MODULE_CHILDREN_FILENAME),
        false => transpile(TranspileInput {
            document: &document,
            component_name: &component_name,
            theme_type: options.theme_type,
            assets: options.assets,
            hot_reload: options.flavour.is_hot(),
            previews: options.flavour.has_previews(),
        }),
    };
    let source = generated.map_err(|source| PackageError::Codegen {
        path: rsx_path.clone(),
        source,
    })?;
    Ok(GeneratedFile {
        rsx_path,
        source_hash,
        rel_out,
        source,
    })
}

#[cfg(feature = "transpile")]
/// Writes each file's Rust and its `.rs.map` sidecar under `generated_dir`, returning every `.rs` path written.
///
/// The returned set is what tells live output from an orphan: anything else under the directory belongs to a `.rsx` that was renamed or deleted, which is [`crate::prune_stale_generated`]'s input.
pub fn write_package(
    files: &[GeneratedFile],
    generated_dir: &Path,
) -> Result<HashSet<PathBuf>, PackageError> {
    let mut written = HashSet::new();
    for file in files {
        let out_path = file.out_path(generated_dir);
        if let Some(parent) = out_path.parent() {
            write_error(&out_path, std::fs::create_dir_all(parent))?;
        }
        write_error(
            &out_path,
            telar_project::write_if_changed_atomic(&out_path, &file.source.rust_code),
        )?;
        // Beside the build file, so the editor extension and `cargo telar check` can put a diagnostic on the generated Rust back onto the `.rsx` line — and the column — the author wrote.
        let map = SourceMap::new(
            file.source.source_map.clone(),
            file.source.expr_spans.clone(),
        );
        let map_path = out_path.with_extension("rs.map");
        write_error(
            &map_path,
            telar_project::write_if_changed_atomic(&map_path, &map.to_json()),
        )?;
        written.insert(out_path);
    }
    Ok(written)
}

#[cfg(feature = "transpile")]
/// The index describing what a transpile produced, for a reader that would rather wire the output than produce it again.
///
/// Written by `cargo telar transpile` beside the generated directory; read by the macro, which compares it against the sources on disk before trusting a line of it.
///
/// **A project that will not install the CLI produces its own**, from a `build.rs` with this crate as a build-dependency — which cargo re-runs on a `.rsx` edit for real, rather than through the `include_str!` a proc macro has to fake it with:
///
/// ```no_run
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let package = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
/// let src_dir = package.join("src");
/// let theme = telar_transpiler::resolve_theme_type(&package);
/// // What the macro compares the index against: the `telar` this project resolves, not this crate's own.
/// let workspace = telar_project::find_workspace_root(&package).unwrap_or_else(|| package.clone());
/// let telar_version = telar_project::resolve_telar_version(&workspace).ok_or("no telar dependency")?;
/// let assets = telar_project::AssetContext::load(&package, &telar_version);
///
/// let flavour = telar_project::BuildFlavour::Plain;
/// let files = telar_transpiler::transpile_package(&telar_transpiler::PackageOptions {
///     src_dir: &src_dir,
///     theme_type: theme.as_deref(),
///     assets: Some(&assets),
///     flavour,
/// })?;
/// telar_transpiler::write_package(&files, &telar_project::generated_dir(&package, flavour))?;
/// let index = telar_transpiler::build_index(&files, &src_dir, theme.as_deref(), "build.rs", &telar_version);
/// telar_project::write_build_index(&package, flavour, &index)?;
///
/// for file in &files {
///     println!("cargo:rerun-if-changed={}", file.rsx_path.display());
/// }
/// # Ok(())
/// # }
/// ```
pub fn build_index(
    files: &[GeneratedFile],
    src_dir: &Path,
    theme_type: Option<&str>,
    producer: &str,
    telar_version: &str,
) -> BuildIndex {
    BuildIndex {
        format: BUILD_ARTIFACT_FORMAT,
        producer: producer.to_string(),
        telar_version: telar_version.to_string(),
        theme: theme_type.map(str::to_string),
        uses_assets: files
            .iter()
            .any(|file| file.source.rust_code.contains(telar_project::ASSETS_MODULE)),
        entries: files
            .iter()
            .filter_map(|file| {
                Some(BuildEntry {
                    source: relative_source(&file.rsx_path, src_dir)?,
                    hash: file.source_hash.clone(),
                    output_hash: telar_project::content_hash(file.source.rust_code.as_bytes()),
                    previews: !file.source.preview_names.is_empty(),
                })
            })
            .collect(),
    }
}

#[cfg(feature = "transpile")]
fn write_error(path: &Path, result: std::io::Result<()>) -> Result<(), PackageError> {
    result.map_err(|source| PackageError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(all(test, feature = "transpile"))]
#[path = "package_test.rs"]
mod tests;

/// The artifact's own tests live here rather than beside [`BuildIndex`](telar_project::BuildIndex): what they assert is whether an index still answers for sources it was written from, and producing one means running a real transpile.
#[cfg(test)]
#[path = "build_artifact_test.rs"]
mod artifact_tests;
