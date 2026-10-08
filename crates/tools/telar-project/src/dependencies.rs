//! What a package's `Cargo.toml` says about the tooling crates `cargo telar` turns on for it.
//!
//! A tooling crate such as `telar-devtools` reaches an application as an optional dependency the application declares itself. The CLI names its feature on the build, and `telar::app!` names the crate under `cfg(feature = "…")`, but only when this module answers that the package declares one: a package that does not builds exactly as if the crate did not exist.

use std::collections::BTreeSet;
use std::path::Path;

use crate::cargo_manifest::{dependency_tables, read_manifest};

/// The devtools overlay `cargo telar dev` turns on, as the application's `Cargo.toml` names it and as the feature of the same name.
pub const DEVTOOLS_PACKAGE: &str = "telar-devtools";

/// Whether the package at `package_dir` declares `dependency` as an optional dependency that a feature of the same name turns on. A `Cargo.toml` that cannot be read declares nothing.
///
/// The feature has to be one that turns the dependency on, or code gated on it would name a crate the build never compiled. Before the 2024 edition, Cargo gives an optional dependency an implicit feature of its own name unless some feature activates it as `dep:<name>`; from 2024 on, only an explicit feature exists, which is what `cargo add --optional` writes.
pub fn declares_optional_dependency(package_dir: &Path, dependency: &str) -> bool {
    let Some(manifest) = read_manifest(package_dir) else {
        return false;
    };
    let implicit_features = edition(&manifest, package_dir).is_some_and(|edition| edition < 2024);
    declares(&manifest, implicit_features, dependency)
}

/// The package's edition, through `edition.workspace = true` when it inherits one, and 2015 when it names none. `None` when an inherited edition cannot be found.
fn edition(manifest: &toml::Table, package_dir: &Path) -> Option<u32> {
    let declared = manifest.get("package")?.get("edition");
    let spelled = match declared {
        None => return Some(2015),
        Some(toml::Value::String(edition)) => edition.clone(),
        Some(_) => {
            let root = crate::find_workspace_root(package_dir)?;
            read_manifest(&root)?
                .get("workspace")?
                .get("package")?
                .get("edition")?
                .as_str()?
                .to_string()
        }
    };
    spelled.parse().ok()
}

fn declares(manifest: &toml::Table, implicit_features: bool, dependency: &str) -> bool {
    if !is_optional_dependency(manifest, dependency) {
        return false;
    }
    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    let activation = format!("dep:{dependency}");
    if !features.contains_key(dependency) {
        return implicit_features
            && !features
                .values()
                .flat_map(feature_list)
                .any(|feature| feature == activation);
    }
    enabled_by(&features, dependency).contains(&activation)
}

fn is_optional_dependency(manifest: &toml::Table, dependency: &str) -> bool {
    dependency_tables(manifest, "dependencies")
        .filter_map(|table| table.get(dependency))
        .any(|declared| {
            declared
                .get("optional")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false)
        })
}

fn enabled_by(features: &toml::Table, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![root.to_string()];
    while let Some(feature) = queue.pop() {
        if !seen.insert(feature.clone()) {
            continue;
        }
        if let Some(value) = features.get(&feature) {
            queue.extend(feature_list(value));
        }
    }
    seen
}

fn feature_list(value: &toml::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
#[path = "dependencies_test.rs"]
mod tests;
