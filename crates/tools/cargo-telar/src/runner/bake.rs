//! `cargo telar bake` and the single point that runs it before every subcommand that goes on to compile:
//! walking each workspace member's `.rsx` files for `src:"…"` asset references and baking them into
//! `.telar/assets.rs`/`assets.json`, through `telar-baker` rather than the transpiler's own in-macro baker.
//! That is what keeps `usvg`/`resvg`/`image` (~66 crates) out of every project's own build — they compile
//! once, into this binary, instead of once per proc-macro invocation of every project that uses `telar`.
//!
//! Nothing reads the artifact yet: `telar-macros` still bakes on its own (`telar-transpiler`'s
//! `view/media.rs`), so this only adds files nobody consumes. Wiring the macro to read them instead is a
//! later task.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use telar_parser::{RsxDocument, ViewNode};
use telar_transpiler::{AssetKind, BakedAsset, asset_kind_for_tag, assets_root};

use super::config::{CargoManifest, expand_member, find_package_dir};

/// Bakes every workspace member's `.rsx` asset references. Called once from [`super::run`], before
/// dispatching to any subcommand that goes on to invoke `cargo` — never from inside `watch.rs`,
/// `android.rs`, or any of `cargo-telar`'s other `cargo` call sites, so a bake never runs more than once
/// per invocation and none of those sites had to learn about baking at all.
pub(crate) fn bake_workspace() {
    let dir = find_package_dir(&[]);
    let workspace_root = telar_transpiler::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let telar_version = resolve_telar_version(&workspace_root);
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));
    for member in member_dirs(&workspace_root) {
        bake_package(&member, &producer, &telar_version);
    }
}

/// Every crate directory under `workspace_root`: the root's own package (a workspace manifest may name
/// both `[workspace]` and `[package]`) plus every `[workspace] members` glob, expanded and filtered down to
/// entries that are actually a crate — a glob segment matches plain directories too. Falls back to treating
/// `workspace_root` itself as the sole member when its manifest declares no `[workspace]` at all, which is
/// every project `cargo telar new` scaffolds.
fn member_dirs(workspace_root: &Path) -> Vec<PathBuf> {
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

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
}

#[derive(Deserialize)]
struct CargoMetadataPackage {
    name: String,
    version: String,
}

/// The `telar`/`telar-macros` version the baked `assets.rs` is written against: the version `telar`
/// actually resolves to for `workspace_root`, not this binary's own — a project can pin an older `telar`
/// than whatever `cargo-telar` baked it, and `AssetIndex::telar_version` exists precisely to catch that.
/// `--no-deps` would miss it whenever `telar` is a published dependency rather than a workspace member (as
/// in every project but this one), so this runs the full resolve. Falls back to this binary's own version —
/// itself correct for this repository, where every one of these crates shares the workspace version — when
/// `cargo metadata` cannot be run at all or names no `telar` dependency.
fn resolve_telar_version(workspace_root: &Path) -> String {
    Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| serde_json::from_slice::<CargoMetadata>(&output.stdout).ok())
        .and_then(|metadata| {
            metadata
                .packages
                .into_iter()
                .find(|pkg| pkg.name == "telar")
        })
        .map(|pkg| pkg.version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Bakes one package's asset references, writing `<package_dir>/.telar/assets.{json,rs}` — or leaving the
/// package untouched if it has no `.rsx` at all, so a crate with nothing to bake never grows a `.telar/`.
fn bake_package(package_dir: &Path, producer: &str, telar_version: &str) {
    let rsx_files = telar_transpiler::find_rsx_files_in_tree(package_dir);
    if rsx_files.is_empty() {
        return;
    }

    let mut refs: Vec<(&'static AssetKind, String)> = Vec::new();
    for rsx in &rsx_files {
        let Ok(source) = std::fs::read_to_string(rsx) else {
            continue;
        };
        let Ok(doc) = telar_parser::parse(&source) else {
            continue;
        };
        collect_asset_refs(&doc, &mut refs);
    }

    let assets_root = assets_root(package_dir);
    let telar_dir = package_dir.join(".telar");
    let previous_index = telar_transpiler::read_index(&telar_dir).ok().flatten();
    let previous_source =
        std::fs::read_to_string(telar_dir.join(telar_transpiler::ASSETS_SOURCE_FILENAME)).ok();

    let mut baked: Vec<BakedAsset> = Vec::new();
    // Keyed on path alone, matching `generate_assets`'s own uniqueness rule: two tags naming one file resolve to one entry rather than failing the whole package.
    let mut seen_paths = BTreeSet::new();
    for (kind, path) in refs {
        let path = path.replace('\\', "/");
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        let file_path = assets_root.join(&path);
        let bytes = match std::fs::read(&file_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!(
                    "[cargo-telar] warning: cannot bake {} asset `{path}`: not found at {} ({e})",
                    kind.label,
                    file_path.display()
                );
                continue;
            }
        };
        let hash = telar_transpiler::content_hash(&bytes);

        let cached_expr = previous_index
            .as_ref()
            .zip(previous_source.as_deref())
            .and_then(|(index, source)| {
                let entry = index
                    .entries
                    .iter()
                    .find(|e| e.path == path && e.kind == kind.id && e.hash == hash)?;
                init_expr_for_static(source, &entry.static_name)
            });

        let init_expr = match cached_expr {
            Some(expr) => expr,
            None => {
                let Some(baker) = telar_baker::baker_for_id(kind.id) else {
                    eprintln!(
                        "[cargo-telar] warning: no baker registered for asset kind `{}`",
                        kind.id
                    );
                    continue;
                };
                match baker.bake(&bytes) {
                    Ok(expr) => expr,
                    Err(e) => {
                        eprintln!(
                            "[cargo-telar] warning: cannot bake {} asset `{path}`: {e}",
                            kind.label
                        );
                        continue;
                    }
                }
            }
        };

        baked.push(BakedAsset {
            kind: kind.id.to_string(),
            path,
            content: bytes,
            init_expr,
        });
    }

    let generated = match telar_transpiler::generate_assets(&baked, producer, telar_version) {
        Ok(generated) => generated,
        Err(e) => {
            eprintln!(
                "[cargo-telar] warning: could not bake assets for {}: {e}",
                package_dir.display()
            );
            return;
        }
    };

    let changed = previous_index.as_ref() != Some(&generated.index);
    if let Err(e) = telar_transpiler::write_generated(&telar_dir, &generated) {
        eprintln!(
            "[cargo-telar] warning: could not write {}: {e}",
            telar_dir.display()
        );
        return;
    }
    if changed {
        let name = package_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("package");
        eprintln!(
            "[cargo-telar] Baked {} asset(s) for {name}",
            generated.index.entries.len()
        );
    }
}

/// The Rust expression a previous bake already wrote for `static_name`, reused so an asset whose content
/// hash is unchanged is never re-decoded through `usvg`/`resvg`/`image` on every `cargo telar bake`. Parses
/// the exact single-line shape [`telar_transpiler::generate_assets`] emits for one entry — brittle to that
/// shape changing, which is exactly what bumping `ASSET_ARTIFACT_FORMAT` is for.
fn init_expr_for_static(source: &str, static_name: &str) -> Option<String> {
    let marker = format!("pub static {static_name}: ");
    let line = source.lines().find(|line| line.starts_with(&marker))?;
    let start = line.find("Arc::new(")? + "Arc::new(".len();
    let end = line.rfind("));")?;
    (start <= end).then(|| line[start..end].to_string())
}

/// Every `kind.attr` reference an `.rsx` document's view (and its `[preview]` bodies) makes to a static,
/// quoted asset path. Mirrors `telar-analyzer`'s `document_links` walk, minus the LSP-only concerns: a
/// dynamic `src:$signal`/`src:expr` names no file to bake, so only `Value::Quoted` is collected.
fn collect_asset_refs(doc: &RsxDocument, out: &mut Vec<(&'static AssetKind, String)>) {
    collect_nodes(&doc.view.nodes, out);
    for preview in &doc.previews {
        collect_nodes(&preview.body, out);
    }
}

fn collect_nodes(nodes: &[ViewNode], out: &mut Vec<(&'static AssetKind, String)>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                if let Some(kind) = asset_kind_for_tag(&el.tag) {
                    for attr in &el.attributes {
                        if attr.key == kind.attr && attr.value.is_quoted() {
                            let text = attr.value.text().trim();
                            if !text.is_empty() {
                                out.push((kind, text.to_string()));
                            }
                        }
                    }
                }
                collect_nodes(&el.children, out);
            }
            ViewNode::IfBlock(block) => {
                collect_nodes(&block.then_branch, out);
                if let Some(else_branch) = &block.else_branch {
                    collect_nodes(else_branch, out);
                }
            }
            ViewNode::ForBlock(block) => collect_nodes(&block.body, out),
            ViewNode::MatchBlock(block) => {
                for arm in &block.arms {
                    collect_nodes(&arm.body, out);
                }
            }
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON_SVG: &[u8] = include_bytes!("../../../telar-baker/tests/fixtures/icon.svg");
    const DOT_PNG: &[u8] = include_bytes!("../../../telar-baker/tests/fixtures/dot.png");

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cargo_telar_bake_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn init_expr_round_trips_a_real_baked_entry() {
        let expected = telar_baker::baker_for_id("svg")
            .unwrap()
            .bake(ICON_SVG)
            .unwrap();
        let baked = BakedAsset {
            kind: "svg".to_string(),
            path: "badge.svg".to_string(),
            content: ICON_SVG.to_vec(),
            init_expr: expected.clone(),
        };
        let generated =
            telar_transpiler::generate_assets(std::slice::from_ref(&baked), "p", "1.0.0").unwrap();
        let static_name = telar_transpiler::static_name_for_path("badge.svg");

        assert_eq!(
            init_expr_for_static(&generated.source, &static_name).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn init_expr_is_none_for_a_static_the_source_never_declared() {
        assert!(init_expr_for_static("", "ASSET_MISSING").is_none());
    }

    #[test]
    fn collects_static_refs_including_previews_and_skips_dynamic_or_empty_ones() {
        let src = "[view]\ncol\n    \
                   svg src:\"badge.svg\"\n    \
                   img src:$dynamic\n    \
                   img src:\"\"\n\
                   [preview \"demo\"]\ncol\n    img src:\"dot.png\"\n";
        let doc = telar_parser::parse(src).unwrap();
        let mut refs = Vec::new();
        collect_asset_refs(&doc, &mut refs);
        let paths: Vec<_> = refs
            .iter()
            .map(|(kind, path)| (kind.id, path.as_str()))
            .collect();
        assert_eq!(paths, vec![("svg", "badge.svg"), ("image", "dot.png")]);
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

    #[test]
    fn bakes_only_the_assets_a_document_actually_references() {
        let root = temp_dir("bake_pkg");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/badge.svg"), ICON_SVG).unwrap();
        std::fs::write(root.join("assets/dot.png"), DOT_PNG).unwrap();
        std::fs::write(root.join("assets/unused.svg"), ICON_SVG).unwrap();
        std::fs::write(
            root.join("src/view.rsx"),
            "[view]\ncol\n    svg src:\"badge.svg\"\n    img src:\"dot.png\"\n",
        )
        .unwrap();

        bake_package(&root, "test-producer", "9.9.9");

        let index = telar_transpiler::read_index(&root.join(".telar"))
            .unwrap()
            .expect("a package with references bakes an index");
        assert_eq!(index.producer, "test-producer");
        assert_eq!(index.telar_version, "9.9.9");
        let mut paths: Vec<_> = index.entries.iter().map(|e| e.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, vec!["badge.svg", "dot.png"]);

        let source = std::fs::read_to_string(root.join(".telar/assets.rs")).unwrap();
        assert!(source.contains("SvgData"), "{source}");
        assert!(source.contains("ImageData"), "{source}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_asset_file_is_skipped_with_a_warning_not_a_panic() {
        let root = temp_dir("bake_missing");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(
            root.join("src/view.rsx"),
            "[view]\ncol\n    img src:\"missing.png\"\n",
        )
        .unwrap();

        bake_package(&root, "p", "1.0.0");

        let index = telar_transpiler::read_index(&root.join(".telar")).unwrap();
        assert!(index.is_none_or(|i| i.entries.is_empty()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_package_with_no_rsx_is_left_untouched() {
        let root = temp_dir("bake_no_rsx");
        std::fs::create_dir_all(&root).unwrap();

        bake_package(&root, "p", "1.0.0");

        assert!(!root.join(".telar").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
