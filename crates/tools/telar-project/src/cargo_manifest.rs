//! The one reader of a package's `Cargo.toml`, for the questions that ask what it depends on.

use std::path::Path;

pub(crate) fn read_manifest(dir: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()
}

/// Every table of the `kind` (`dependencies`, `dev-dependencies`, `build-dependencies`) in `manifest`: the top-level one first, then each `[target.*.<kind>]` copy.
pub(crate) fn dependency_tables<'a>(
    manifest: &'a toml::Table,
    kind: &'a str,
) -> impl Iterator<Item = &'a toml::Table> {
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(move |target| target.get(kind));
    std::iter::once(manifest.get(kind))
        .flatten()
        .chain(targets)
        .filter_map(toml::Value::as_table)
}
