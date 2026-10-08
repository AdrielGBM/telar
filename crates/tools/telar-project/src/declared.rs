//! A `telar.toml` list entry together with the line that declares it, for the checks that answer about a key's value rather than about the generated code it shapes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cargo_manifest::{dependency_tables, read_manifest};

/// One entry of a `telar.toml` list, and where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredEntry<T> {
    pub entry: T,
    pub file: PathBuf,
    /// 1-based.
    pub line: usize,
    /// The character columns of the entry's string, quotes included, within [`Self::line`].
    pub columns: (usize, usize),
}

/// An entry the package cannot use, said about the line that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProblem<T> {
    pub declaration: DeclaredEntry<T>,
    pub message: String,
    pub help: String,
}

/// The lists of strings whose lines a check reports against.
#[derive(Deserialize)]
struct SpannedManifest {
    telar: Option<SpannedTelar>,
}

#[derive(Deserialize)]
pub(crate) struct SpannedTelar {
    pub(crate) prelude: Option<Vec<toml::Spanned<String>>>,
    pub(crate) previews: Option<SpannedPreviews>,
}

#[derive(Deserialize)]
pub(crate) struct SpannedPreviews {
    pub(crate) include: Option<Vec<toml::Spanned<String>>>,
}

/// The entries of the list `select` picks out of the `telar.toml` [`crate::TelarManifest::load`] reads it from for `package_dir`: the package's own, or the workspace's when the package inherits it. Entries `parse` refuses are left out, since loading the manifest already refused them.
pub(crate) fn declarations<T>(
    package_dir: &Path,
    select: fn(SpannedTelar) -> Option<Vec<toml::Spanned<String>>>,
    parse: fn(&str) -> Result<T, String>,
) -> Result<Vec<DeclaredEntry<T>>, crate::ManifestError> {
    crate::TelarManifest::load(package_dir)?;
    for file in crate::TelarManifest::files(package_dir) {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let spanned: SpannedManifest =
            toml::from_str(&content).map_err(|e| crate::ManifestError::Invalid {
                path: file.clone(),
                message: e.to_string(),
            })?;
        let Some(list) = spanned.telar.and_then(select) else {
            continue;
        };
        return Ok(list
            .iter()
            .filter_map(|declared| {
                let entry = parse(declared.get_ref()).ok()?;
                let span = declared.span();
                let line_start = content[..span.start].rfind('\n').map_or(0, |at| at + 1);
                let column = |at: usize| content[line_start..at].chars().count();
                Some(DeclaredEntry {
                    entry,
                    file: file.clone(),
                    line: content[..span.start].matches('\n').count() + 1,
                    columns: (column(span.start), column(span.end)),
                })
            })
            .collect());
    }
    Ok(Vec::new())
}

/// What `package_dir`'s `Cargo.toml` says about the crates it may name, or `None` when it cannot be read.
pub(crate) struct PackageDependencies {
    pub(crate) package: String,
    manifest: toml::Table,
    normal: HashSet<String>,
}

impl PackageDependencies {
    pub(crate) fn read(package_dir: &Path) -> Option<Self> {
        let manifest = read_manifest(package_dir)?;
        let package = manifest
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("this package")
            .to_string();
        let normal = dependency_crates(&manifest, "dependencies");
        Some(Self {
            package,
            manifest,
            normal,
        })
    }

    /// Whether `root` is a crate the package's own code can name.
    pub(crate) fn reaches(&self, root: &str) -> bool {
        self.normal.contains(root) || ALWAYS_LINKED.contains(&root)
    }

    /// The package's own crate name, as Rust spells it.
    pub(crate) fn crate_name(&self) -> String {
        self.package.replace('-', "_")
    }

    /// The help for an entry naming `root`, which the package does not list under `[dependencies]`, where `reader` is what would fail to name it.
    pub(crate) fn missing_help(&self, root: &str, reader: &str) -> String {
        let package_name = root.replace('_', "-");
        let elsewhere = ["dev-dependencies", "build-dependencies"]
            .into_iter()
            .find(|table| dependency_crates(&self.manifest, table).contains(root));
        match elsewhere {
            Some(table) => format!(
                "`{root}` is under `[{table}]`, which {reader} cannot reach: move it to `[dependencies]` (`cargo add {package_name}`)"
            ),
            None => format!(
                "add it with `cargo add -p {} {package_name}`, or remove the entry",
                self.package
            ),
        }
    }
}

/// Crates every Rust program can name without declaring them.
const ALWAYS_LINKED: [&str; 3] = ["std", "core", "alloc"];

/// The crate names Rust sees for one dependency table of `manifest`, its target-specific copies included. A renamed dependency is named by its key, which is what `package = "…"` exists to choose.
fn dependency_crates(manifest: &toml::Table, table: &str) -> HashSet<String> {
    dependency_tables(manifest, table)
        .flat_map(|dependencies| dependencies.keys())
        .map(|key| key.replace('-', "_"))
        .collect()
}
