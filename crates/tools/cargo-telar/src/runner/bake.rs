//! `cargo telar bake` and the single point that runs it before every subcommand that goes on to compile.
//!
//! Only the workspace half lives here — which directories are members, and which `telar` the project resolves. Baking one package is `telar-baker`'s, because `telar-analyzer` bakes too and neither binary can depend on the other.

use std::path::{Path, PathBuf};

use super::config::{CargoManifest, expand_member, find_package_dir};

/// Bakes every workspace member's `.rsx` asset references and translation catalogs, answering whether every member baked without an error.
///
/// Called once from [`super::run`], before dispatching to any subcommand that goes on to invoke `cargo`, and again from each of `watch.rs`'s two loops, which spawn their own `cargo` per rebuild. **Every place in this binary that spawns `cargo` is a build route**, and a build route that has not baked compiles against a stale artifact — or, since the macro checks hashes, fails. A new one either calls this first or sits downstream of a call that did.
///
/// An error is printed here and left to the caller to act on: a one-off command exits on it, while a watch loop goes on to the build, whose `compile_error!` points at the `.rsx` line, and waits for the next edit.
///
/// Members bake after the members they are built with, and each is told every crate it is built with, so its icon notice merges the records those crates' bakes leave.
pub(crate) fn bake_workspace() -> bool {
    let mut clean = true;
    let dir = find_package_dir(&[]);
    let workspace_root = telar_project::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let metadata = telar_project::cargo_metadata(&workspace_root);
    // This binary's own version is the right fallback only because every crate here shares the workspace version; for any other project a failed resolve means cargo itself is unusable, and the build is about to say so.
    let telar_version = metadata
        .as_ref()
        .and_then(telar_project::telar_version_in)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    super::config::warn_if_foreign_version(&telar_version);
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));
    let graph = metadata
        .as_ref()
        .map(telar_baker::DependencyGraph::from_metadata);
    let members = member_dirs(&workspace_root);
    let members = match &graph {
        Some(graph) => graph.bake_order(members),
        None => members,
    };
    for member in members {
        let name = member
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package");
        let dependencies = graph
            .as_ref()
            .and_then(|graph| graph.icon_dependencies(&member));
        if let Some(report) = telar_baker::bake_package_with(
            &member,
            &producer,
            &telar_version,
            dependencies.as_deref(),
        ) {
            for warning in &report.warnings {
                eprintln!("[cargo-telar] warning: {warning}");
            }
            if report.changed {
                eprintln!("[cargo-telar] Baked {} asset(s) for {name}", report.baked);
            }
            for error in &report.errors {
                eprintln!("[cargo-telar] error: {name}: {error}");
            }
            clean &= report.errors.is_empty();
        }
        if let Some(report) = telar_baker::bake_catalog(&member, &producer, &telar_version) {
            for warning in &report.warnings {
                eprintln!("[cargo-telar] warning: {warning}");
            }
            if report.changed {
                eprintln!(
                    "[cargo-telar] Baked {} translation key(s) for {name}",
                    report.keys
                );
            }
        }
    }
    clean
}

pub(super) fn member_dirs(workspace_root: &Path) -> Vec<PathBuf> {
    let manifest = std::fs::read_to_string(workspace_root.join("Cargo.toml"))
        .ok()
        .and_then(|content| toml::from_str::<CargoManifest>(&content).ok());

    let mut dirs = Vec::new();
    let mut has_workspace = false;
    if let Some(manifest) = manifest {
        if manifest.package.is_some() {
            dirs.push(workspace_root.to_path_buf());
        }
        if let Some(workspace) = manifest.workspace {
            has_workspace = true;
            for pattern in &workspace.members {
                dirs.extend(
                    expand_member(workspace_root, pattern)
                        .into_iter()
                        .filter(|member| member.join("Cargo.toml").is_file()),
                );
            }
        }
    }
    if !has_workspace && dirs.is_empty() {
        dirs.push(workspace_root.to_path_buf());
    }
    dirs
}

#[cfg(test)]
#[path = "bake_test.rs"]
mod tests;
