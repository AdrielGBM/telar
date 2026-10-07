//! The icons of the crates a package is built with: the record a `[telar] library` ships of what it baked, the crates of cargo's dependency graph that can carry baked icons, and what the package's own licence policy makes of their sets.
//!
//! A dependency's icons are baked into its own artifact and compiled into the application with it, so the notice the application ships has to list them although its own bake never resolved them. A library is read-only wherever cargo put it, a registry copy included, so it ships the record of its icons in its package; a local crate the workspace bakes is read from the record its own bake left.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use icons_core::{LicensePolicy, Verdict};
use serde::{Deserialize, Serialize};
use telar_project::{ICONS_LIBRARY_RECORD_FILENAME, TelarManifest};

use crate::icons::{IconRecord, RecordedSet, read_icon_record};

const LIBRARY_RECORD_FORMAT: u32 = 1;

/// The icons one crate baked, by id, and what each set they came from says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CrateIcons {
    pub sets: BTreeMap<String, RecordedSet>,
    pub icons: BTreeSet<String>,
}

impl CrateIcons {
    /// The package's own icons in `record`, without those of the crates it is built with, and without where the bake read each one.
    pub fn own(record: &IconRecord) -> Self {
        Self {
            sets: record.sets.clone(),
            icons: record.icons.keys().cloned().collect(),
        }
    }

    /// The names drawn from the set `prefix`, sorted.
    pub(crate) fn names_in(&self, prefix: &str) -> Vec<String> {
        self.icons
            .iter()
            .filter_map(|id| id.split_once(':'))
            .filter(|(set, _)| *set == prefix)
            .map(|(_, name)| name.to_string())
            .collect()
    }

    fn absorb(&mut self, other: CrateIcons) {
        for (prefix, set) in other.sets {
            self.sets.entry(prefix).or_insert(set);
        }
        self.icons.extend(other.icons);
    }
}

/// What [`ICONS_LIBRARY_RECORD_FILENAME`] holds.
#[derive(Serialize, Deserialize)]
struct LibraryRecord {
    format: u32,
    #[serde(flatten)]
    icons: CrateIcons,
}

/// Writes the record a library ships of its own icons, or removes it when `icons` is `None`.
pub(crate) fn write_library_record(
    telar_dir: &Path,
    icons: Option<CrateIcons>,
) -> std::io::Result<()> {
    let path = telar_dir.join(ICONS_LIBRARY_RECORD_FILENAME);
    let Some(icons) = icons else {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    };
    let record = LibraryRecord {
        format: LIBRARY_RECORD_FORMAT,
        icons,
    };
    let json = serde_json::to_string_pretty(&record).map_err(std::io::Error::other)?;
    telar_project::write_if_changed_atomic(&path, &format!("{json}\n"))
}

/// The icons the library at `library_dir` ships a record of, checked against the icons its baked artifact holds: `Ok(None)` when it baked none, and an error saying what is wrong when the record is missing, unreadable or does not answer for the artifact.
///
/// What `cargo telar package` checks before a library ships, and what an application's bake reads it by.
pub fn read_library_icons(library_dir: &Path) -> Result<Option<CrateIcons>, String> {
    let telar_dir = library_dir.join(".telar");
    let baked: BTreeSet<String> = telar_project::read_index(&telar_dir)
        .ok()
        .flatten()
        .map(|index| {
            index
                .entries
                .into_iter()
                .filter(|entry| entry.kind == "icon")
                .map(|entry| entry.path)
                .collect()
        })
        .unwrap_or_default();
    let shown = format!(".telar/{ICONS_LIBRARY_RECORD_FILENAME}");
    let record = match std::fs::read_to_string(telar_dir.join(ICONS_LIBRARY_RECORD_FILENAME)) {
        Ok(text) => Some(
            serde_json::from_str::<LibraryRecord>(&text)
                .map_err(|e| format!("`{shown}` cannot be read: {e}"))?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("`{shown}` cannot be read: {e}")),
    };
    match record {
        None if baked.is_empty() => Ok(None),
        None => Err(format!(
            "the artifact holds {} baked icon(s) ({}) and there is no `{shown}` naming the sets and licences they come from",
            baked.len(),
            baked.iter().cloned().collect::<Vec<_>>().join(", ")
        )),
        Some(record) if record.format != LIBRARY_RECORD_FORMAT => Err(format!(
            "`{shown}` is in format {}, and this bake reads format {LIBRARY_RECORD_FORMAT}",
            record.format
        )),
        Some(record) if record.icons.icons != baked => Err(format!(
            "`{shown}` lists other icons than the {} the artifact holds, so it was not written by the bake that baked them",
            baked.len()
        )),
        Some(record) => Ok(Some(record.icons).filter(|icons| !icons.icons.is_empty())),
    }
}

/// A crate a package is built with whose baked icons ship in what the package builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconDependency {
    pub name: String,
    pub dir: PathBuf,
    /// A `[telar] library`, read through the record it ships. Any other is a local crate, read through the record its own bake left.
    pub library: bool,
}

impl IconDependency {
    fn icons(&self) -> Result<Option<CrateIcons>, String> {
        if self.library {
            return read_library_icons(&self.dir);
        }
        Ok(read_icon_record(&self.dir)
            .map(|record| CrateIcons::own(&record))
            .filter(|icons| !icons.icons.is_empty()))
    }
}

/// A workspace's dependency graph as `cargo metadata` resolves it, reduced to what the icon notice needs: which crates can carry baked icons and which crates each one is built with.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    nodes: Vec<Node>,
    by_dir: HashMap<PathBuf, usize>,
}

#[derive(Debug)]
struct Node {
    name: String,
    dir: PathBuf,
    library: bool,
    /// Without a registry or git source, so a bake of the workspace's or of its own may have left a record.
    local: bool,
    /// The crates it is built with: normal dependencies, not those only its build script or its tests use.
    deps: Vec<usize>,
}

impl DependencyGraph {
    /// The graph `metadata`, the output of `cargo metadata --format-version 1` with its resolve, describes.
    pub fn from_metadata(metadata: &serde_json::Value) -> Self {
        let mut graph = Self::default();
        let mut by_id: HashMap<&str, usize> = HashMap::new();
        for package in metadata["packages"].as_array().into_iter().flatten() {
            let (Some(id), Some(name), Some(dir)) = (
                package["id"].as_str(),
                package["name"].as_str(),
                package["manifest_path"]
                    .as_str()
                    .and_then(|path| Path::new(path).parent()),
            ) else {
                continue;
            };
            let index = graph.nodes.len();
            by_id.insert(id, index);
            graph.by_dir.insert(canonical(dir), index);
            graph.nodes.push(Node {
                name: name.to_string(),
                dir: dir.to_path_buf(),
                library: is_library(dir),
                local: package["source"].is_null(),
                deps: Vec::new(),
            });
        }
        for node in metadata["resolve"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(&index) = node["id"].as_str().and_then(|id| by_id.get(id)) else {
                continue;
            };
            graph.nodes[index].deps = node["deps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|dep| {
                    dep["dep_kinds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|kind| kind["kind"].is_null())
                })
                .filter_map(|dep| by_id.get(dep["pkg"].as_str()?).copied())
                .collect();
        }
        graph
    }

    /// Every crate the package at `package_dir` is built with, at any depth, that can carry baked icons, sorted by name. `None` when the graph does not hold the package.
    pub fn icon_dependencies(&self, package_dir: &Path) -> Option<Vec<IconDependency>> {
        let root = *self.by_dir.get(&canonical(package_dir))?;
        let mut seen = BTreeSet::from([root]);
        let mut pending = vec![root];
        let mut found = Vec::new();
        while let Some(index) = pending.pop() {
            for &dep in &self.nodes[index].deps {
                if !seen.insert(dep) {
                    continue;
                }
                pending.push(dep);
                let node = &self.nodes[dep];
                if node.library || node.local {
                    found.push(IconDependency {
                        name: node.name.clone(),
                        dir: node.dir.clone(),
                        library: node.library,
                    });
                }
            }
        }
        found.sort_by(|a, b| (&a.name, &a.dir).cmp(&(&b.name, &b.dir)));
        Some(found)
    }

    /// `members` in an order that bakes every member after the members it is built with, since a member's notice is merged from the records their bakes leave. Members the graph does not hold keep their place relative to each other, at the end.
    pub fn bake_order(&self, members: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut known: Vec<(usize, PathBuf)> = Vec::new();
        let mut unknown = Vec::new();
        for member in members {
            match self.by_dir.get(&canonical(&member)) {
                Some(&index) => known.push((index, member)),
                None => unknown.push(member),
            }
        }
        let wanted: HashMap<usize, PathBuf> = known.iter().cloned().collect();
        let mut visited = BTreeSet::new();
        let mut ordered = Vec::new();
        for (index, _) in &known {
            self.visit(*index, &wanted, &mut visited, &mut ordered);
        }
        ordered.extend(unknown);
        ordered
    }

    fn visit(
        &self,
        index: usize,
        wanted: &HashMap<usize, PathBuf>,
        visited: &mut BTreeSet<usize>,
        ordered: &mut Vec<PathBuf>,
    ) {
        if !visited.insert(index) {
            return;
        }
        for &dep in &self.nodes[index].deps {
            self.visit(dep, wanted, visited, ordered);
        }
        if let Some(member) = wanted.get(&index) {
            ordered.push(member.clone());
        }
    }
}

fn is_library(dir: &Path) -> bool {
    dir.join(telar_project::MANIFEST_FILENAME).is_file()
        && TelarManifest::load(dir).is_ok_and(|manifest| manifest.telar.library)
}

fn canonical(dir: &Path) -> PathBuf {
    dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf())
}

/// The icons of the crates a package is built with, each judged against the package's `policy`: read from `dependencies` when the caller could ask cargo for them, and otherwise the ones `previous` recorded, so a bake that cannot ask, such as the editor's, leaves the notice as the last full bake wrote it.
///
/// A set the policy refuses is an error, since those icons are already baked into a crate the package cannot rebake, and are kept in the record because they still ship.
pub(crate) fn dependency_icons(
    previous: Option<&IconRecord>,
    dependencies: Option<&[IconDependency]>,
    policy: &LicensePolicy,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) -> BTreeMap<String, CrateIcons> {
    let icons = match dependencies {
        None => previous
            .map(|record| record.dependencies.clone())
            .unwrap_or_default(),
        Some(dependencies) => {
            let mut icons: BTreeMap<String, CrateIcons> = BTreeMap::new();
            for dependency in dependencies {
                match dependency.icons() {
                    Ok(Some(found)) => icons
                        .entry(dependency.name.clone())
                        .or_default()
                        .absorb(found),
                    Ok(None) => {}
                    Err(problem) => warnings.push(format!(
                        "the icons `{}` draws are left out of this package's licence notice: in {}, {problem}. A library has to be packaged with a `cargo telar` that ships its icon record",
                        dependency.name,
                        dependency.dir.display()
                    )),
                }
            }
            icons
        }
    };
    for (name, crate_icons) in &icons {
        for (prefix, set) in &crate_icons.sets {
            let verdict = policy.judge(prefix, set.info.as_ref(), set.own);
            let attributed = |message: String| {
                format!(
                    "`{name}`, which this package is built with, ships icons this package's licence policy has not accepted: {message}"
                )
            };
            match verdict {
                Verdict::Accepted => {}
                Verdict::Warn(message) => warnings.push(attributed(message)),
                Verdict::Fail(message) => errors.push(attributed(message)),
            }
        }
    }
    icons
}

#[cfg(test)]
#[path = "icon_dependencies_test.rs"]
mod tests;
