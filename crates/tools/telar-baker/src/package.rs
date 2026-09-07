//! Baking one package's `.rsx` asset references into its `.telar/` artifact.
//!
//! Lives here rather than in `cargo-telar` because two binaries need it and neither can depend on the other: the CLI bakes while building an app, and `telar-analyzer` bakes so the editor stops showing an error for a `src:"…"` that no build has reached yet. Two copies of this would drift, and a drift here means the IDE and the compiler disagree about what a project's assets are.
//!
//! What stays out is workspace layout — which directories are members, which package the user meant. That is the CLI's question; this module is told a package directory and answers for it.

use std::collections::BTreeSet;
use std::path::Path;

use telar_parser::{RsxDocument, ViewNode};
use telar_project::{AssetKind, BakedAsset, asset_kind_for_tag, assets_root};

/// What baking one package turned up. Warnings are collected rather than printed so each caller renders them where its user is looking — a terminal for the CLI, the LSP log for the analyzer — and none of them is fatal: a package with one unreadable asset still bakes the rest.
#[derive(Debug, Clone, Default)]
pub struct BakeReport {
    pub baked: usize,
    /// Whether the index differs from what was already on disk. `false` means nothing was rewritten, which is what keeps a bake from retriggering rustc or the editor's file watcher.
    pub changed: bool,
    pub warnings: Vec<String>,
}

/// Bakes every asset `package_dir`'s `.rsx` files reference, writing `<package_dir>/.telar/assets.{json,rs}`. `None` when the package holds no `.rsx` at all, so a crate with nothing to bake never grows a `.telar/`.
///
/// `telar_version` is the version of `telar` the *project* resolves — see [`telar_project::resolve_telar_version`]. Writing this binary's own version here would hand the macro a mismatch it cannot act on.
pub fn bake_package(package_dir: &Path, producer: &str, telar_version: &str) -> Option<BakeReport> {
    let rsx_files = telar_project::find_rsx_files_in_tree(package_dir);
    if rsx_files.is_empty() {
        return None;
    }

    let mut report = BakeReport::default();
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
    let previous_index = telar_project::read_index(&telar_dir).ok().flatten();
    let previous_source =
        std::fs::read_to_string(telar_dir.join(telar_project::ASSETS_SOURCE_FILENAME)).ok();

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
                report.warnings.push(format!(
                    "cannot bake {} asset `{path}`: not found at {} ({e})",
                    kind.label,
                    file_path.display()
                ));
                continue;
            }
        };
        let hash = telar_project::content_hash(&bytes);

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
                let Some(baker) = super::baker_for_id(kind.id) else {
                    report
                        .warnings
                        .push(format!("no baker registered for asset kind `{}`", kind.id));
                    continue;
                };
                match baker.bake(&bytes) {
                    Ok(expr) => expr,
                    Err(e) => {
                        report
                            .warnings
                            .push(format!("cannot bake {} asset `{path}`: {e}", kind.label));
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

    let generated = match telar_project::generate_assets(&baked, producer, telar_version) {
        Ok(generated) => generated,
        Err(e) => {
            report.warnings.push(format!(
                "could not bake assets for {}: {e}",
                package_dir.display()
            ));
            return Some(report);
        }
    };

    report.changed = previous_index.as_ref() != Some(&generated.index);
    report.baked = generated.index.entries.len();
    if let Err(e) = telar_project::write_generated(&telar_dir, &generated) {
        report
            .warnings
            .push(format!("could not write {}: {e}", telar_dir.display()));
        report.changed = false;
    }
    Some(report)
}

/// The Rust expression a previous bake already wrote for `static_name`, reused so an asset whose content hash is unchanged is never re-decoded through `usvg`/`resvg`/`image` on every bake. Parses the exact single-line shape [`telar_project::generate_assets`] emits for one entry — brittle to that shape changing, which is exactly what bumping `ASSET_ARTIFACT_FORMAT` is for.
fn init_expr_for_static(source: &str, static_name: &str) -> Option<String> {
    let marker = format!("pub static {static_name}: ");
    let line = source.lines().find(|line| line.starts_with(&marker))?;
    let start = line.find("Arc::new(")? + "Arc::new(".len();
    let end = line.rfind("));")?;
    (start <= end).then(|| line[start..end].to_string())
}

/// Every `kind.attr` reference an `.rsx` document's view (and its `[preview]` bodies) makes to a static, quoted asset path. Mirrors `telar-analyzer`'s `document_links` walk, minus the LSP-only concerns: a dynamic `src:$signal`/`src:expr` names no file to bake, so only `Value::Quoted` is collected.
pub fn collect_asset_refs(doc: &RsxDocument, out: &mut Vec<(&'static AssetKind, String)>) {
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
#[path = "package_test.rs"]
mod tests;
