//! Baking one package's `.rsx` asset references into its `.telar/` artifact.
//!
//! Lives here rather than in `cargo-telar` because two binaries need it and neither can depend on the other: the CLI bakes while building an app, and `telar-analyzer` bakes so the editor stops showing an error for a `src:"…"` that no build has reached yet. Two copies of this would drift, and a drift here means the IDE and the compiler disagree about what a project's assets are.
//!
//! What stays out is workspace layout — which directories are members, which package the user meant. That is the CLI's question; this module is told a package directory and answers for it.

use std::collections::BTreeSet;
use std::path::Path;

use telar_parser::{RsxDocument, ViewNode};
use telar_project::{
    ASSET_KINDS, AssetContext, AssetIndex, AssetKind, BakedAsset, IconsSection, IdBaking,
    TelarManifest, asset_kind_for_tag, assets_root,
};

use crate::icon_dependencies::IconDependency;
use crate::icons::{Recording, read_icon_record, resolve_with};
use crate::ids::{IdRef, collect_id_refs};

/// What baking one package turned up. Warnings and errors are collected rather than printed so each caller renders them where its user is looking — a terminal for the CLI, the LSP log for the analyzer. A warning is not fatal: a package with one unreadable asset still bakes the rest.
#[derive(Debug, Clone, Default)]
pub struct BakeReport {
    pub baked: usize,
    /// Whether the index differs from what was already on disk. `false` means nothing was rewritten, which is what keeps a bake from retriggering rustc or the editor's file watcher.
    pub changed: bool,
    pub warnings: Vec<String>,
    /// What a build must not go on from: an id that has to be baked and is not a literal, one no source has, a set the licence policy refuses. Everything else is still baked, so an editor keeps answering for the rest.
    pub errors: Vec<String>,
}

/// Bakes every asset `package_dir`'s `.rsx` files reference, writing `<package_dir>/.telar/assets.{json,rs}`. `None` when the package holds no `.rsx` and ships no icon of the crates it is built with, so a crate with nothing to bake never grows a `.telar/`.
///
/// That is every `src:"…"` file, and every literal id given to a component-named kind's prop where the package bakes that kind: the icons `[telar.icons]` resolves, judged against its licence policy and recorded beside the artifact.
///
/// `telar_version` is the version of `telar` the *project* resolves — see [`telar_project::resolve_telar_version`]. Writing this binary's own version here would hand the macro a mismatch it cannot act on.
///
/// The icon notice keeps the icons of the crates the package is built with as the last bake that knew them recorded; [`bake_package_with`] is the bake that is told them.
pub fn bake_package(package_dir: &Path, producer: &str, telar_version: &str) -> Option<BakeReport> {
    bake_package_with(package_dir, producer, telar_version, None)
}

/// [`bake_package`], told the crates the package is built with, as [`DependencyGraph::icon_dependencies`](crate::DependencyGraph::icon_dependencies) finds them, so its notice lists their icons beside its own. `None` keeps the ones the last bake recorded, for a caller that cannot ask cargo every time it bakes, such as the editor.
///
/// A package with no `.rsx` bakes nothing of its own, and still writes the notice of what the crates it is built with baked, since it ships their icons.
pub fn bake_package_with(
    package_dir: &Path,
    producer: &str,
    telar_version: &str,
    dependencies: Option<&[IconDependency]>,
) -> Option<BakeReport> {
    let rsx_files = telar_project::find_rsx_files_in_tree(package_dir);
    let manifest = TelarManifest::load_or_default(package_dir);
    let recording = Recording {
        library: manifest.telar.library,
        dependencies,
    };
    if rsx_files.is_empty() {
        if dependencies.is_none_or(<[IconDependency]>::is_empty)
            && read_icon_record(package_dir).is_none()
        {
            return None;
        }
        let icons = resolve_with(
            package_dir,
            &icons_section(&manifest, false),
            &[],
            recording,
        );
        if icons.warnings.is_empty()
            && icons.errors.is_empty()
            && read_icon_record(package_dir).is_none()
        {
            return None;
        }
        return Some(BakeReport {
            warnings: icons.warnings,
            errors: icons.errors,
            ..BakeReport::default()
        });
    }

    let id_kinds: Vec<(&'static AssetKind, IdBaking)> = ASSET_KINDS
        .iter()
        .map(|kind| (kind, manifest.telar.id_baking(kind)))
        .filter(|(_, baking)| *baking != IdBaking::Off)
        .collect();
    let id_kind_list: Vec<&'static AssetKind> = id_kinds.iter().map(|(kind, _)| *kind).collect();

    let mut report = BakeReport::default();
    let mut refs: Vec<(&'static AssetKind, String)> = Vec::new();
    let mut id_refs: Vec<IdRef> = Vec::new();
    for rsx in &rsx_files {
        let Ok(source) = std::fs::read_to_string(rsx) else {
            continue;
        };
        let Ok(doc) = telar_parser::parse(&source) else {
            continue;
        };
        collect_asset_refs(&doc, &mut refs);
        collect_id_refs(&doc, rsx, &id_kind_list, &mut id_refs);
    }

    let assets_root = assets_root(package_dir);
    let telar_dir = package_dir.join(".telar");
    let previous_index = telar_project::read_index(&telar_dir).ok().flatten();
    let previous = Previous {
        // An entry baked in another format is not an expression this one can reuse, however unchanged its file.
        index: previous_index
            .as_ref()
            .filter(|index| index.format == telar_project::ASSET_ARTIFACT_FORMAT),
        source: std::fs::read_to_string(telar_dir.join(telar_project::ASSETS_SOURCE_FILENAME)).ok(),
    };

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
        baked.extend(bake_one(kind, path, bytes, false, &previous, &mut report));
    }

    for (kind, baking) in &id_kinds {
        if *baking == IdBaking::Required {
            for reference in id_refs
                .iter()
                .filter(|reference| reference.kind.id == kind.id && reference.literal.is_none())
            {
                let message = AssetContext::dynamic_id_message(kind);
                report.errors.push(format!(
                    "{}: `{}:{}` — {}",
                    reference.location(package_dir),
                    kind.attr,
                    reference.written,
                    message.trim_start_matches("rsx: ")
                ));
            }
        }
    }
    let icon_refs: Vec<IdRef> = id_refs
        .into_iter()
        .filter(|reference| {
            reference
                .kind
                .component
                .is_some_and(|c| c.section == "icons")
        })
        .collect();
    let bakes_icons = id_kinds.iter().any(|(kind, _)| kind.id == "icon");
    let icons = resolve_with(
        package_dir,
        &icons_section(&manifest, bakes_icons),
        &icon_refs,
        recording,
    );
    report.warnings.extend(icons.warnings);
    report.errors.extend(icons.errors);
    let icon_kind =
        telar_project::asset_kind_for_id("icon").expect("icon is a registered asset kind");
    for icon in icons.icons {
        baked.extend(bake_one(
            icon_kind,
            icon.id.to_string(),
            icon.svg,
            icon.monochrome,
            &previous,
            &mut report,
        ));
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

/// The `[telar.icons]` the bake resolves the package's own ids through, which names no source unless the package bakes icons. Its licence policy holds either way, since it also judges the icons of the crates the package is built with.
fn icons_section(manifest: &TelarManifest, bakes_icons: bool) -> IconsSection {
    match manifest.telar.icons.clone() {
        Some(section) if bakes_icons => section,
        Some(section) => IconsSection {
            licenses: section.licenses,
            ..IconsSection::default()
        },
        None => IconsSection::default(),
    }
}

/// The artifact a previous bake left, consulted so an asset whose content is unchanged keeps the expression already written for it.
struct Previous<'a> {
    index: Option<&'a AssetIndex>,
    source: Option<String>,
}

/// One asset as the artifact will hold it: the expression a previous bake wrote for these exact bytes, or a fresh bake of them. `None`, with a warning, when the bytes do not bake.
fn bake_one(
    kind: &'static AssetKind,
    path: String,
    bytes: Vec<u8>,
    monochrome: bool,
    previous: &Previous<'_>,
    report: &mut BakeReport,
) -> Option<BakedAsset> {
    let hash = telar_project::content_hash(&bytes);
    let cached_expr = previous
        .index
        .zip(previous.source.as_deref())
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
                return None;
            };
            match baker.bake(&bytes) {
                Ok(expr) => expr,
                Err(e) => {
                    report
                        .warnings
                        .push(format!("cannot bake {} asset `{path}`: {e}", kind.label));
                    return None;
                }
            }
        }
    };

    Some(BakedAsset {
        kind: kind.id.to_string(),
        path,
        content: bytes,
        init_expr,
        monochrome,
    })
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
