//! Resolving the icon ids a package bakes through the sources `[telar.icons]` names, judging each set's licence against the package's policy, and recording what was baked under which licence.
//!
//! Three files land in `.telar/` beside the asset artifact: `icons.json`, the record of every baked icon's set and source and of the icons the crates the package is built with baked; `ICONS-LICENSES.txt`, the notice an application ships, listing both; and `icons/<set>/<name>.svg`, a copy of each icon a provider answered, so a rebake does not ask the network again for what it already has. A `[telar] library` also writes `icons-library.json`, the record of its own icons that it ships.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use icons_core::{
    HttpProvider, IconError, IconId, IconSource, IconifyDir, LicensePolicy, NoticeSet, OnUnlisted,
    SetInfo, SourcedIcon, Sources, SvgDir, Verdict,
};
use serde::{Deserialize, Serialize};
use telar_project::{ICONS_NOTICE_FILENAME, IconsSection, UnlistedLicense};

use crate::icon_dependencies::{
    CrateIcons, IconDependency, dependency_icons, write_library_record,
};
use crate::ids::IdRef;

/// File name of the record, joined onto a package's `.telar/`.
pub const ICONS_RECORD_FILENAME: &str = "icons.json";
const PROVIDER_CACHE_DIR: &str = "icons";
const RECORD_FORMAT: u32 = 1;

/// The icons a package's literal ids resolved to, and what the resolution had to say.
#[derive(Default)]
pub(crate) struct ResolvedIcons {
    pub icons: Vec<BakedIcon>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// One icon to bake: its id, its SVG document, and whether it takes the colour around it, decided from its set's `palette` and its markup by [`SourcedIcon::monochrome`].
#[derive(Debug, PartialEq)]
pub(crate) struct BakedIcon {
    pub id: IconId,
    pub svg: Vec<u8>,
    pub monochrome: bool,
}

/// What `.telar/icons.json` holds: where each baked icon came from and what its set says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IconRecord {
    pub format: u32,
    /// The provider the bake could fetch from, which a cached copy is only reused under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub sets: BTreeMap<String, RecordedSet>,
    pub icons: BTreeMap<String, RecordedIcon>,
    /// The icons of the crates the package is built with, by crate name, which its notice lists after its own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, CrateIcons>,
}

/// What the bake knows of a package beyond its own `.rsx`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Recording<'a> {
    /// Whether the package is a `[telar] library`, which ships the record of its own icons for the applications built with it.
    pub library: bool,
    /// The crates the package is built with, or `None` to keep the ones the previous record holds.
    pub dependencies: Option<&'a [IconDependency]>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordedSet {
    pub own: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<SetInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordedIcon {
    pub origin: String,
    #[serde(default)]
    pub from_provider: bool,
}

/// Reads a package's icon record, `None` when it has none or it cannot be read.
pub fn read_icon_record(package_dir: &Path) -> Option<IconRecord> {
    let text =
        std::fs::read_to_string(package_dir.join(".telar").join(ICONS_RECORD_FILENAME)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Resolves every literal id in `refs`, then judges, records and notices what resolved, together with the icons of the crates `recording` says the package is built with. Neither holding an icon clears the record and the notice, so a package that stopped drawing icons stops shipping their notice.
pub(crate) fn resolve_with(
    package_dir: &Path,
    section: &IconsSection,
    refs: &[IdRef],
    recording: Recording<'_>,
) -> ResolvedIcons {
    let telar_dir = package_dir.join(".telar");
    let mut out = ResolvedIcons::default();

    let mut wanted: BTreeMap<IconId, &IdRef> = BTreeMap::new();
    for reference in refs {
        let Some(literal) = &reference.literal else {
            continue;
        };
        match section.icon_id(literal) {
            Ok(id) => {
                wanted.entry(id).or_insert(reference);
            }
            Err(error) => out
                .errors
                .push(format!("{}: {error}", reference.location(package_dir))),
        }
    }
    let ids: Vec<IconId> = wanted.keys().cloned().collect();

    let mut local = Sources::new();
    if let Some(dir) = section.svg_dir(package_dir) {
        local = local.with(SvgDir::new(dir));
    }
    if let Some(dir) = section.iconify_dir(package_dir) {
        local = local.with(IconifyDir::new(dir));
    }
    let provider = section
        .provider
        .as_deref()
        .map(|base| HttpProvider::new(base.trim()));
    let previous = read_icon_record(package_dir);

    let mut answers: Vec<Result<Option<SourcedIcon>, IconError>> = local.icons(&ids);
    let mut from_provider: BTreeSet<IconId> = BTreeSet::new();
    if let Some(provider) = &provider {
        let open: Vec<usize> = (0..ids.len())
            .filter(|&i| matches!(answers[i], Ok(None)))
            .collect();
        let mut to_fetch = Vec::new();
        for i in open {
            match cached(&telar_dir, previous.as_ref(), provider.base(), &ids[i]) {
                Some(icon) => {
                    answers[i] = Ok(Some(icon));
                    from_provider.insert(ids[i].clone());
                }
                None => to_fetch.push(i),
            }
        }
        if !to_fetch.is_empty() {
            let asked: Vec<IconId> = to_fetch.iter().map(|&i| ids[i].clone()).collect();
            for (i, answer) in to_fetch.into_iter().zip(provider.icons(&asked)) {
                if let Ok(Some(icon)) = &answer {
                    keep_in_cache(&telar_dir, &ids[i], &icon.svg);
                    from_provider.insert(ids[i].clone());
                }
                answers[i] = answer;
            }
        }
    }

    let described = match (&provider, local.is_empty()) {
        (Some(provider), true) => provider.describe(),
        (Some(provider), false) => format!("{}, then {}", local.describe(), provider.describe()),
        (None, _) => local.describe(),
    };
    let mut resolved: Vec<(IconId, SourcedIcon)> = Vec::new();
    for (id, answer) in ids.into_iter().zip(answers) {
        let location = wanted[&id].location(package_dir);
        match answer {
            Ok(Some(icon)) => resolved.push((id, icon)),
            Ok(None) => out.errors.push(format!(
                "{location}: no icon source has `{id}`: it is in none of {described}"
            )),
            Err(error) => out
                .errors
                .push(format!("{location}: could not resolve `{id}`: {error}")),
        }
    }

    let policy = LicensePolicy {
        allow: section.licenses.allow.clone(),
        unlisted: match section.unlisted_license() {
            UnlistedLicense::Warn => OnUnlisted::Warn,
            UnlistedLicense::Fail => OnUnlisted::Fail,
        },
    };
    let mut sets: BTreeMap<String, RecordedSet> = BTreeMap::new();
    for (id, icon) in &resolved {
        sets.entry(id.prefix().to_string())
            .or_insert_with(|| RecordedSet {
                own: icon.own,
                info: icon.set.clone(),
            });
    }
    let mut refused: BTreeSet<String> = BTreeSet::new();
    for (prefix, set) in &sets {
        match policy.judge(prefix, set.info.as_ref(), set.own) {
            Verdict::Accepted => {}
            Verdict::Warn(message) => out.warnings.push(message),
            Verdict::Fail(message) => {
                out.errors.push(message);
                refused.insert(prefix.clone());
            }
        }
    }
    resolved.retain(|(id, _)| !refused.contains(id.prefix()));
    sets.retain(|prefix, _| !refused.contains(prefix));

    let record = IconRecord {
        format: RECORD_FORMAT,
        provider: provider
            .as_ref()
            .map(|provider| provider.base().to_string()),
        icons: resolved
            .iter()
            .map(|(id, icon)| {
                (
                    id.to_string(),
                    RecordedIcon {
                        origin: icon.origin.clone(),
                        from_provider: from_provider.contains(id),
                    },
                )
            })
            .collect(),
        sets,
        dependencies: dependency_icons(
            previous.as_ref(),
            recording.dependencies,
            &policy,
            &mut out.warnings,
            &mut out.errors,
        ),
    };
    if let Err(e) = write_record(package_dir, &record, recording.library) {
        out.warnings.push(format!(
            "could not write the icon record in {}: {e}",
            telar_dir.display()
        ));
    }
    prune_cache(&telar_dir, &from_provider);

    out.icons = resolved
        .into_iter()
        .map(|(id, icon)| BakedIcon {
            monochrome: icon.monochrome(),
            svg: icon.svg.into_bytes(),
            id,
        })
        .collect();
    out
}

/// A copy of `id` an earlier bake fetched from this same provider, so the bake needs no network for it.
fn cached(
    telar_dir: &Path,
    previous: Option<&IconRecord>,
    provider: &str,
    id: &IconId,
) -> Option<SourcedIcon> {
    let previous = previous.filter(|record| record.provider.as_deref() == Some(provider))?;
    let recorded = previous
        .icons
        .get(&id.to_string())
        .filter(|icon| icon.from_provider)?;
    let set = previous.sets.get(id.prefix())?;
    let svg = std::fs::read_to_string(cache_path(telar_dir, id)).ok()?;
    Some(SourcedIcon {
        svg,
        set: set.info.clone(),
        own: set.own,
        origin: recorded.origin.clone(),
    })
}

fn cache_path(telar_dir: &Path, id: &IconId) -> PathBuf {
    telar_dir
        .join(PROVIDER_CACHE_DIR)
        .join(id.prefix())
        .join(format!("{}.svg", id.name()))
}

fn keep_in_cache(telar_dir: &Path, id: &IconId, svg: &str) {
    let path = cache_path(telar_dir, id);
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_ok()
    {
        let _ = telar_project::write_if_changed_atomic(&path, svg);
    }
}

/// Removes every cached copy no icon of this bake came from, and every set directory left empty.
fn prune_cache(telar_dir: &Path, kept: &BTreeSet<IconId>) {
    let root = telar_dir.join(PROVIDER_CACHE_DIR);
    let Ok(sets) = std::fs::read_dir(&root) else {
        return;
    };
    for set in sets.flatten() {
        let Ok(files) = std::fs::read_dir(set.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let id = path
                .file_stem()
                .and_then(|name| name.to_str())
                .zip(set.file_name().to_str())
                .and_then(|(name, prefix)| IconId::new(prefix, name).ok());
            if !id.is_some_and(|id| kept.contains(&id)) {
                let _ = std::fs::remove_file(&path);
            }
        }
        let _ = std::fs::remove_dir(set.path());
    }
    let _ = std::fs::remove_dir(&root);
}

/// Writes the record and the notice, or removes both when nothing was baked.
fn write_record(package_dir: &Path, record: &IconRecord, library: bool) -> std::io::Result<()> {
    let telar_dir = package_dir.join(".telar");
    let record_path = telar_dir.join(ICONS_RECORD_FILENAME);
    let notice_path = telar_dir.join(ICONS_NOTICE_FILENAME);
    let own = CrateIcons::own(record);
    if own.icons.is_empty() && record.dependencies.is_empty() {
        for path in [record_path, notice_path] {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        return write_library_record(&telar_dir, None);
    }
    std::fs::create_dir_all(&telar_dir)?;
    write_library_record(
        &telar_dir,
        Some(own.clone()).filter(|own| library && !own.icons.is_empty()),
    )?;
    let json = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    telar_project::write_if_changed_atomic(&record_path, &format!("{json}\n"))?;
    telar_project::write_if_changed_atomic(
        &notice_path,
        &notice_text(&package_name(package_dir), &own, &record.dependencies),
    )
}

/// The notice of the package's own icons, then one for each crate it is built with that baked any, each naming the crate the icons were baked into.
fn notice_text(
    package: &str,
    own: &CrateIcons,
    dependencies: &BTreeMap<String, CrateIcons>,
) -> String {
    let own = (!own.icons.is_empty()).then_some((package, own));
    own.into_iter()
        .chain(
            dependencies
                .iter()
                .map(|(name, icons)| (name.as_str(), icons)),
        )
        .map(|(name, icons)| icons_core::notice(name, &notice_sets(icons)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn notice_sets(icons: &CrateIcons) -> Vec<NoticeSet> {
    icons
        .sets
        .iter()
        .map(|(prefix, set)| NoticeSet {
            prefix: prefix.clone(),
            set: set.info.clone(),
            own: set.own,
            icons: icons.names_in(prefix),
        })
        .collect()
}

fn package_name(package_dir: &Path) -> String {
    std::fs::read_to_string(package_dir.join("Cargo.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|manifest| {
            manifest
                .get("package")?
                .get("name")?
                .as_str()
                .map(str::to_string)
        })
        .or_else(|| {
            package_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "this application".to_string())
}

#[cfg(test)]
#[path = "icons_test.rs"]
mod tests;
