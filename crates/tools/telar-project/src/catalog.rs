//! The pre-baked i18n catalog artifact, and resolving a `t!` key against it.
//!
//! Same split as the asset artifact, and for the same reason: `.telar/i18n.rs` is the generated module holding the catalog, and `.telar/i18n.json` is the index that answers "does this key exist, and what arguments does it take" without re-parsing every locale file. The macro reads the index; nothing here parses TOML, which is what lets `telar-transpiler` — a crate every app compiles into its proc macro — stop carrying `i18n-core` and the reactive runtime it drags along.
//!
//! Producing the artifact is `telar-baker`'s job. This module defines the shape both sides agree on, reads it, and phrases what to do when it cannot be read.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::assets::content_hash;

/// Bumped when `.telar/i18n.rs`'s generated source changes shape in a way an older `telar` cannot read. Independent of the asset artifact's own format: the two are written by the same command but consumed by different code, and coupling their versions would force a re-bake of one for a change to the other.
pub const CATALOG_ARTIFACT_FORMAT: u32 = 1;

/// The crate-root module the baked catalog is wired under, and the path every `t!`/markup call site references.
pub const I18N_MODULE: &str = "__rsx_i18n";
/// Where generated code reaches the baked catalogue.
pub const I18N_CATALOG_PATH: &str = "crate::__rsx_i18n::CATALOG";

/// File name of the generated index, joined onto a package's `.telar/` directory.
pub const CATALOG_INDEX_FILENAME: &str = "i18n.json";
/// File name of the generated Rust module, joined onto a package's `.telar/` directory.
pub const CATALOG_SOURCE_FILENAME: &str = "i18n.rs";

/// One translatable key and the placeholders its message expects, which is the whole of what `t!` validates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    /// Sorted, so a message rewritten with its placeholders in another order does not rewrite the index.
    pub args: Vec<String>,
}

/// One locale file the catalog was baked from, with the hash that says whether it has changed since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSourceFile {
    /// Path relative to the package root, `/`-separated so a project baked on Windows and on Linux agree.
    pub path: String,
    pub hash: String,
}

/// The index `.telar/i18n.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogIndex {
    pub format: u32,
    pub producer: String,
    /// The `telar` version the generated source was written against — see [`crate::AssetIndex::telar_version`], which this mirrors exactly.
    pub telar_version: String,
    /// Sorted by key, so adding a translation is an insertion in the diff rather than a reshuffle.
    pub entries: Vec<CatalogEntry>,
    /// Sorted by path. What the macro turns into `include_str!` triggers, and what a staleness check compares against.
    pub sources: Vec<CatalogSourceFile>,
}

impl CatalogIndex {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .expect("CatalogIndex holds only strings and Vecs of them")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("malformed catalog index: {e}"))
    }
}

/// Reads `<telar_dir>/i18n.json`. `Ok(None)` when the file does not exist. A malformed file is `Err`, not `Ok(None)`: "never baked" and "baked something unreadable" want different reactions, so they must not look the same.
pub fn read_catalog_index(telar_dir: &Path) -> std::io::Result<Option<CatalogIndex>> {
    match std::fs::read_to_string(telar_dir.join(CATALOG_INDEX_FILENAME)) {
        Ok(json) => CatalogIndex::from_json(&json)
            .map(Some)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// What a package's `.telar/` had to say about its catalog. `Unusable` carries the mismatch already phrased, so a crate with three hundred `t!` calls does not re-derive it three hundred times.
enum CatalogState {
    NotBaked,
    Unusable(String),
    Ready(CatalogIndex),
}

/// One package's baked catalog, loaded once and consulted per `t!`.
pub struct CatalogContext {
    package_dir: PathBuf,
    telar_dir: PathBuf,
    telar_version: String,
    state: CatalogState,
}

impl CatalogContext {
    /// Reads `<package_dir>/.telar/i18n.json` and checks it against this build's format and `current_telar_version`. Never fails: a package with no `t!` in it must not be broken by an artifact it never consults, so every problem is reported by the call site that runs into it.
    pub fn load(package_dir: &Path, current_telar_version: &str) -> Self {
        let telar_dir = package_dir.join(".telar");
        let state = match read_catalog_index(&telar_dir) {
            Ok(None) => CatalogState::NotBaked,
            Ok(Some(index)) if index.format != CATALOG_ARTIFACT_FORMAT => {
                CatalogState::Unusable(format!(
                    "it is in format {}, and this build of telar reads format {CATALOG_ARTIFACT_FORMAT}",
                    index.format
                ))
            }
            Ok(Some(index)) if index.telar_version != current_telar_version => {
                CatalogState::Unusable(format!(
                    "it was baked for telar {}, and this project builds telar {current_telar_version}",
                    index.telar_version
                ))
            }
            Ok(Some(index)) => CatalogState::Ready(index),
            Err(e) => CatalogState::Unusable(format!("it could not be read: {e}")),
        };
        Self {
            package_dir: package_dir.to_path_buf(),
            telar_dir,
            telar_version: current_telar_version.to_string(),
            state,
        }
    }

    /// The loaded index, or `None` when the artifact is missing or unusable.
    pub fn index(&self) -> Option<&CatalogIndex> {
        match &self.state {
            CatalogState::Ready(index) => Some(index),
            _ => None,
        }
    }

    /// The generated module to wire in as `#[path] pub mod __rsx_i18n;`, or `None` when there is nothing usable to wire.
    pub fn module_file(&self) -> Option<PathBuf> {
        self.index()?;
        let path = self.telar_dir.join(CATALOG_SOURCE_FILENAME);
        path.is_file().then_some(path)
    }

    /// The index file itself, so a caller that read it can make its build depend on it.
    pub fn index_file(&self) -> Option<PathBuf> {
        let path = self.telar_dir.join(CATALOG_INDEX_FILENAME);
        path.is_file().then_some(path)
    }

    /// Every locale file the catalog was baked from, for a caller to register as a build input.
    pub fn tracked_files(&self) -> Vec<PathBuf> {
        self.index()
            .map(|index| {
                index
                    .sources
                    .iter()
                    .map(|source| self.package_dir.join(&source.path))
                    .filter(|path| path.is_file())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The placeholders `key` expects, or the message explaining why the question cannot be answered. `Ok(None)` means the artifact is readable and simply does not define `key` — an unknown key, which the caller reports against the key's own span.
    pub fn arg_names(&self, key: &str) -> Result<Option<&[String]>, String> {
        let index = match &self.state {
            CatalogState::NotBaked => return Err(self.not_baked_message()),
            CatalogState::Unusable(why) => return Err(self.unusable_message(why)),
            CatalogState::Ready(index) => index,
        };
        if let Some(stale) = self.stale_source() {
            return Err(stale);
        }
        Ok(index
            .entries
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.args.as_slice()))
    }

    /// Whether the catalog defines no keys at all, which is how "this project has no translations" reads once the artifact exists. Distinct from a missing artifact, which is a build that has not run the baker.
    pub fn is_empty(&self) -> bool {
        self.index().is_some_and(|index| index.entries.is_empty())
    }

    /// The first locale file whose content no longer matches what was baked, phrased for a caller, or `None` when the catalog is current (or absent, which is a different message's job).
    ///
    /// Project-wide, unlike an asset's per-reference check, because the catalog is one input: a changed locale file makes the whole baked `CATALOG` wrong, and no subset of call sites stays correct. So the macro asks this once per expansion rather than leaving it to `t!` — markup `t"…"` emits a `translate` call without validating anything, and a project written entirely in markup would otherwise serve last week's wording with no error at all.
    pub fn staleness(&self) -> Option<String> {
        self.stale_source()
    }

    /// Checked per call rather than at load, because a locale file can change between the two in a `cargo telar dev` session.
    fn stale_source(&self) -> Option<String> {
        let index = self.index()?;
        index.sources.iter().find_map(|source| {
            let bytes = std::fs::read(self.package_dir.join(&source.path)).ok()?;
            (content_hash(&bytes) != source.hash).then(|| {
                format!(
                    "rsx: the translation file `{}` has changed since the catalog was baked. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first).",
                    source.path
                )
            })
        })
    }

    fn not_baked_message(&self) -> String {
        format!(
            "rsx: no baked translation catalog. Catalogs are baked by the CLI, not by the compiler. Run: cargo telar bake (or build through cargo telar check/dev/build, which bake first) — and if this project has no translations yet, create locales/<lang>.toml first. Install the CLI with: cargo install cargo-telar --version {}",
            self.telar_version
        )
    }

    fn unusable_message(&self, why: &str) -> String {
        format!(
            "rsx: the baked translation catalog cannot be used: {why}. Run: cargo install cargo-telar --version {} && cargo telar bake",
            self.telar_version
        )
    }
}

#[cfg(test)]
#[path = "catalog_test.rs"]
mod tests;
