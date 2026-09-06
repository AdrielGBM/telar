//! `cargo telar bake` and the single point that runs it before every subcommand that goes on to compile.
//!
//! Only the workspace half lives here — which directories are members, and which `telar` the project resolves. Baking one package is `telar-baker`'s, because `telar-analyzer` bakes too and neither binary can depend on the other.

use std::path::{Path, PathBuf};

use super::config::{CargoManifest, expand_member, find_package_dir};

/// Bakes every workspace member's `.rsx` asset references. Called once from [`super::run`], before dispatching to any subcommand that goes on to invoke `cargo`, and again from each of `watch.rs`'s two loops, which spawn their own `cargo` per rebuild.
pub(crate) fn bake_workspace() {
    let dir = find_package_dir(&[]);
    let workspace_root = telar_transpiler::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    // This binary's own version is the right fallback only because every crate here shares the workspace version; for any other project a failed resolve means cargo itself is unusable, and the build is about to say so.
    let telar_version = telar_baker::resolve_telar_version(&workspace_root)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));
    for member in member_dirs(&workspace_root) {
        let name = member
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package");
        if let Some(report) = telar_baker::bake_package(&member, &producer, &telar_version) {
            for warning in &report.warnings {
                eprintln!("[cargo-telar] warning: {warning}");
            }
            if report.changed {
                eprintln!("[cargo-telar] Baked {} asset(s) for {name}", report.baked);
            }
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
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cargo_telar_bake_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn member_dirs_expands_globs_and_keeps_only_real_crates() {
        let root = temp_dir("members");
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        for member in ["crates/a", "crates/b"] {
            std::fs::create_dir_all(root.join(member)).unwrap();
            std::fs::write(
                root.join(member).join("Cargo.toml"),
                "[package]\nname = \"x\"\n",
            )
            .unwrap();
        }
        std::fs::create_dir_all(root.join("crates/not_a_crate")).unwrap();

        let mut dirs = member_dirs(&root);
        dirs.sort();
        assert_eq!(dirs, vec![root.join("crates/a"), root.join("crates/b")]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_workspace_manifest_that_is_also_a_package_bakes_itself_too() {
        let root = temp_dir("root_pkg");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"root\"\n\n[workspace]\nmembers = []\n",
        )
        .unwrap();

        assert_eq!(member_dirs(&root), vec![root.clone()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_lone_package_with_no_workspace_table_bakes_itself() {
        let root = temp_dir("lone_pkg");
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();

        assert_eq!(member_dirs(&root), vec![root.clone()]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
