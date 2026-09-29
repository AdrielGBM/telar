//! The pictures a browser build fetches instead of carrying: every `img src:"…"` the app's own crates baked, written at the addresses the bake gave the module.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::WASM_TARGET;
use super::assets::write_new;

/// Writes every picture the crates in `crates` baked, and the smaller copies of each, into `out`. Returns how many files were written.
pub(super) fn ship_images(out: &Path, crates: &[PathBuf]) -> Result<usize, String> {
    let mut written = BTreeSet::new();
    for dir in crates {
        let telar_dir = dir.join(".telar");
        let Some(index) = telar_project::read_index(&telar_dir)
            .map_err(|e| format!("could not read {}: {e}", telar_dir.display()))?
        else {
            continue;
        };
        let root = telar_project::assets_root(dir);
        for entry in index.entries.iter().filter(|entry| entry.kind == "image") {
            let source = root.join(&entry.path);
            let bytes = std::fs::read(&source)
                .map_err(|e| format!("could not read {}: {e}", source.display()))?;
            let web = telar_baker::web_image(&bytes)
                .map_err(|e| format!("could not prepare {} for the web: {e}", source.display()))?;
            let files =
                std::iter::once(web.full).chain(web.copies.into_iter().map(|(_, file)| file));
            for file in files {
                // Named by content, so two crates shipping one picture want one file, not two.
                if written.insert(file.path.clone()) {
                    write_new(&out.join(&file.path), &file.bytes)?;
                }
            }
        }
    }
    Ok(written.len())
}

/// The directories of `package` and of every crate it is built from that is not from a registry or a git source: the ones that can hold `.rsx`, and so a bake. Asked of cargo for the browser target, so a crate only a desktop build pulls in ships nothing here.
pub(super) fn local_crates(workspace_root: &Path, package: &str) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            WASM_TARGET,
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("failed to invoke cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cargo metadata wrote something unreadable: {e}"))?;
    local_crates_in(&metadata, package)
}

pub(super) fn local_crates_in(
    metadata: &serde_json::Value,
    package: &str,
) -> Result<Vec<PathBuf>, String> {
    let local: HashMap<&str, PathBuf> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|pkg| pkg["source"].is_null())
        .filter_map(|pkg| {
            let id = pkg["id"].as_str()?;
            let dir = Path::new(pkg["manifest_path"].as_str()?).parent()?;
            Some((id, dir.to_path_buf()))
        })
        .collect();
    let root = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|pkg| pkg["name"] == package && pkg["source"].is_null())
        .and_then(|pkg| pkg["id"].as_str())
        .ok_or_else(|| format!("cargo metadata does not list the package `{package}`"))?;
    let deps: HashMap<&str, Vec<&str>> = metadata["resolve"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let deps = node["deps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|dep| dep["pkg"].as_str())
                .collect();
            Some((node["id"].as_str()?, deps))
        })
        .collect();
    let mut seen = BTreeSet::from([root]);
    let mut pending = vec![root];
    let mut dirs = Vec::new();
    while let Some(id) = pending.pop() {
        let Some(dir) = local.get(id) else {
            continue;
        };
        dirs.push(dir.clone());
        for dep in deps.get(id).into_iter().flatten() {
            if seen.insert(dep) {
                pending.push(dep);
            }
        }
    }
    dirs.sort();
    Ok(dirs)
}

#[cfg(test)]
#[path = "images_test.rs"]
mod tests;
