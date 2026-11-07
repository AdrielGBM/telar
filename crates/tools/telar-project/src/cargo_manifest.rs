//! The one reader of a package's `Cargo.toml`: every tool that asks what a manifest says asks it here, so none of them can answer with a partial shape of its own.

use std::path::{Path, PathBuf};

use crate::ManifestError;

const CARGO_MANIFEST_FILENAME: &str = "Cargo.toml";

/// A `Cargo.toml` as read from its directory, which the targets cargo discovers on its own are found in.
#[derive(Debug, Clone)]
pub struct CargoManifest {
    dir: PathBuf,
    text: String,
    table: toml::Table,
}

/// The kinds of target a manifest declares with a table of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Lib,
    Bin,
    Test,
    Example,
    Bench,
}

impl TargetKind {
    const ALL: [Self; 5] = [Self::Lib, Self::Bin, Self::Test, Self::Example, Self::Bench];

    fn key(self) -> &'static str {
        match self {
            Self::Lib => "lib",
            Self::Bin => "bin",
            Self::Test => "test",
            Self::Example => "example",
            Self::Bench => "bench",
        }
    }
}

impl CargoManifest {
    /// The manifest in `dir`, or `None` when there is none or it is not TOML.
    pub fn read(dir: &Path) -> Option<Self> {
        Self::load(dir).ok().flatten()
    }

    /// The manifest in `dir`: `Ok(None)` when there is none to read, an error when there is one and it is not TOML.
    pub fn load(dir: &Path) -> Result<Option<Self>, ManifestError> {
        let path = dir.join(CARGO_MANIFEST_FILENAME);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        Self::parse(dir, &text)
            .map(Some)
            .map_err(|e| ManifestError::Invalid {
                path,
                message: e.to_string(),
            })
    }

    /// `text` as the manifest of the package in `dir`.
    pub fn parse(dir: &Path, text: &str) -> Result<Self, toml::de::Error> {
        Ok(Self {
            dir: dir.to_path_buf(),
            table: text.parse()?,
            text: text.to_string(),
        })
    }

    /// The manifest as written, for a question no accessor here answers.
    pub fn table(&self) -> &toml::Table {
        &self.table
    }

    /// The manifest read into `T`, for a caller that wants a typed view of the keys it knows. Read from the text, so an error names the line.
    pub fn view<T: serde::de::DeserializeOwned>(&self) -> Result<T, toml::de::Error> {
        toml::from_str(&self.text)
    }

    /// `[package] name`, or `None` for a virtual workspace manifest.
    pub fn package_name(&self) -> Option<&str> {
        self.table.get("package")?.get("name")?.as_str()
    }

    /// The crate name of the package's library, as `CARGO_CRATE_NAME` spells it: its `[lib] name`, or else the package name with `-` spelled `_`.
    pub fn lib_name(&self) -> Option<String> {
        self.table
            .get("lib")
            .and_then(|lib| lib.get("name"))
            .and_then(toml::Value::as_str)
            .or_else(|| self.package_name())
            .map(|name| name.replace('-', "_"))
    }

    /// Every binary the package builds: each `[[bin]]` by its `name`, then the ones cargo discovers in `src/main.rs`, `src/bin/<name>.rs` and `src/bin/<name>/main.rs` unless `autobins = false`.
    pub fn bin_names(&self) -> Vec<String> {
        let declared = self.targets(TargetKind::Bin);
        let mut names: Vec<String> = declared
            .iter()
            .filter_map(|bin| bin.get("name")?.as_str().map(str::to_string))
            .collect();
        let declared_paths: Vec<PathBuf> = declared
            .iter()
            .filter_map(|bin| bin.get("path")?.as_str())
            .map(|path| self.dir.join(path))
            .collect();
        let autobins = self
            .table
            .get("package")
            .and_then(|package| package.get("autobins"))
            .and_then(toml::Value::as_bool)
            != Some(false);
        if !autobins {
            return names;
        }
        let mut discovered = Vec::new();
        let main = self.dir.join("src/main.rs");
        if main.is_file()
            && let Some(package) = self.package_name()
        {
            discovered.push((package.to_string(), main));
        }
        let bin_dir = self.dir.join("src/bin");
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&bin_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort();
        for path in entries {
            let root = match path.is_dir() {
                true => path.join("main.rs"),
                false => path.clone(),
            };
            let is_rust = root.extension().is_some_and(|ext| ext == "rs");
            if let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
                && is_rust
                && root.is_file()
            {
                discovered.push((name.to_string(), root));
            }
        }
        for (name, path) in discovered {
            if !names.contains(&name) && !declared_paths.contains(&path) {
                names.push(name);
            }
        }
        names
    }

    /// `[package] default-run`: the binary `cargo run` starts when the package builds several.
    pub fn default_run(&self) -> Option<&str> {
        self.table.get("package")?.get("default-run")?.as_str()
    }

    /// `[package] include`, as written.
    pub fn include(&self) -> Option<Vec<&str>> {
        let include = self.table.get("package")?.get("include")?.as_array()?;
        Some(include.iter().filter_map(toml::Value::as_str).collect())
    }

    /// Whether this is the root of a workspace, which a `[workspace]` table says.
    pub fn is_workspace(&self) -> bool {
        self.table.contains_key("workspace")
    }

    /// `[workspace] members`, as written: paths that may hold a `*`.
    pub fn workspace_members(&self) -> Vec<&str> {
        self.table
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .collect()
    }

    /// `[workspace.dependencies]`, which a member's `{ workspace = true }` dependency is completed from.
    pub fn workspace_dependencies(&self) -> Option<&toml::Table> {
        self.table.get("workspace")?.get("dependencies")?.as_table()
    }

    /// Whether the manifest declares `[profile.<name>]`.
    pub fn has_profile(&self, name: &str) -> bool {
        self.table
            .get("profile")
            .and_then(|profiles| profiles.get(name))
            .is_some()
    }

    /// Every table of the `kind` (`dependencies`, `dev-dependencies`, `build-dependencies`): the top-level one first, then each `[target.*.<kind>]` copy.
    pub fn dependency_tables<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a toml::Table> {
        let targets = self
            .table
            .get("target")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|targets| targets.values())
            .filter_map(move |target| target.get(kind));
        std::iter::once(self.table.get(kind))
            .flatten()
            .chain(targets)
            .filter_map(toml::Value::as_table)
    }

    /// The `path` of every target the manifest declares, with its kind, as written.
    pub fn declared_target_paths(&self) -> Vec<(TargetKind, &str)> {
        TargetKind::ALL
            .into_iter()
            .flat_map(|kind| {
                self.targets(kind)
                    .into_iter()
                    .filter_map(move |target| Some((kind, target.get("path")?.as_str()?)))
            })
            .collect()
    }

    fn targets(&self, kind: TargetKind) -> Vec<&toml::Value> {
        match self.table.get(kind.key()) {
            Some(toml::Value::Array(targets)) => targets.iter().collect(),
            Some(target @ toml::Value::Table(_)) => vec![target],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "cargo_manifest.test.rs"]
mod tests;
