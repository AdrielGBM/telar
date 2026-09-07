//! Build-time i18n catalog baker: discovery, parsing, and the artifact both binaries write.
//!
//! Discovers translation files — a project-wide `locales/<tag>.toml` (or `locales/<tag>/*.toml`), and/or per-module `src/**/i18n/<tag>.toml` co-located with each module, all configurable via `[telar.i18n]` in `telar.toml` — parses each into a keyed set of messages, and serializes the merged catalog to a Rust source string of pure `&'static` data.
//!
//! The model and the TOML grammar are `i18n-core`'s, shared with the runtime loader an app without any `.rsx` file uses. What is baker-only is here: discovery, serialization, and the index that lets `t!` validate a key without any of this being linked into the macro.

use std::path::{Path, PathBuf};

use i18n_core::PluralCategory;
use i18n_core::{CatalogModel, MessageModel, PartModel};
use telar_project::{
    CATALOG_ARTIFACT_FORMAT, CATALOG_INDEX_FILENAME, CATALOG_SOURCE_FILENAME, CatalogEntry,
    CatalogIndex, CatalogSourceFile, content_hash, read_catalog_index, write_if_changed_atomic,
};

/// What baking one package's catalog turned up, mirroring [`super::BakeReport`].
#[derive(Debug, Clone, Default)]
pub struct CatalogReport {
    pub keys: usize,
    pub changed: bool,
    pub warnings: Vec<String>,
}

/// Bakes `package_dir`'s translation catalog, writing `<package_dir>/.telar/i18n.{json,rs}`. `None` when the package has neither `.rsx` files nor locale files, so a crate with no stake in either never grows a `.telar/`.
///
/// Note the `or`: `t!` is plain Rust and needs no `.rsx` to appear in — `crates/i18n/i18n-fixture` is exactly that shape — while a package with `.rsx` and no translations still gets an artifact, with no entries. That empty artifact is what lets `t!` tell "this project has no translations" from "nobody has run the baker", which a missing file cannot distinguish.
pub fn bake_catalog(
    package_dir: &Path,
    producer: &str,
    telar_version: &str,
) -> Option<CatalogReport> {
    let sources = catalog_sources(package_dir);
    if sources.is_empty() && telar_project::find_rsx_files_in_tree(package_dir).is_empty() {
        return None;
    }

    let mut report = CatalogReport::default();
    let model = match parse_catalog(package_dir) {
        Ok(model) => model,
        Err(e) => {
            report.warnings.push(format!("catalog: {e}"));
            return Some(report);
        }
    };

    let mut entries: Vec<CatalogEntry> = model
        .iter()
        .flat_map(|model| model.entries.keys())
        .map(|key| CatalogEntry {
            key: key.clone(),
            args: {
                let mut args = model
                    .as_ref()
                    .and_then(|m| m.arg_names(key))
                    .unwrap_or_default();
                args.sort();
                args.dedup();
                args
            },
        })
        .collect();
    entries.sort_by(|a, b| a.key.cmp(&b.key));

    let mut source_files: Vec<CatalogSourceFile> = Vec::new();
    for (_, path) in sources {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let relative = path
            .strip_prefix(package_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        source_files.push(CatalogSourceFile {
            path: relative,
            hash: content_hash(&bytes),
        });
    }
    source_files.sort_by(|a, b| a.path.cmp(&b.path));

    let index = CatalogIndex {
        format: CATALOG_ARTIFACT_FORMAT,
        producer: producer.to_string(),
        telar_version: telar_version.to_string(),
        entries,
        sources: source_files,
    };
    let source = model.as_ref().map(to_source).unwrap_or_default();

    let telar_dir = package_dir.join(".telar");
    let previous = read_catalog_index(&telar_dir).ok().flatten();
    report.changed = previous.as_ref() != Some(&index);
    report.keys = index.entries.len();

    if let Err(e) = std::fs::create_dir_all(&telar_dir)
        .and_then(|()| {
            write_if_changed_atomic(
                &telar_dir.join(CATALOG_INDEX_FILENAME),
                &format!("{}\n", index.to_json()),
            )
        })
        .and_then(|()| write_if_changed_atomic(&telar_dir.join(CATALOG_SOURCE_FILENAME), &source))
    {
        report
            .warnings
            .push(format!("could not write {}: {e}", telar_dir.display()));
        report.changed = false;
    }
    Some(report)
}

/// How the catalog is discovered, read from the one `telar.toml` schema. Both sources may be active at once; either name set to `""` disables it.
struct I18nConfig {
    root: String,
    scan: String,
    default: Option<String>,
}

fn read_i18n_config(package_root: &Path) -> I18nConfig {
    let telar = telar_project::TelarManifest::load_or_default(package_root).telar;
    I18nConfig {
        root: telar
            .locales_root(Path::new(""))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        scan: telar.catalog_scan_dir(),
        default: telar.default_locale(),
    }
}

/// The project-wide catalog directory (`locales/` by default, `[telar.i18n] root`), or `None` when it is disabled. Mirrors [`telar_project::assets_root`]: a caller that needs the *directory* — a file watcher, say — must not have to infer it from the files that happen to be in it today, or adding the first `.toml` to an empty one goes unnoticed. The per-module catalogs the `scan` setting finds are under `src/`, so nothing else needs exposing.
pub fn locales_root(package_root: &Path) -> Option<PathBuf> {
    let root = read_i18n_config(package_root).root;
    (!root.is_empty()).then(|| package_root.join(root))
}

/// The locale files that feed the catalog, sorted — used to emit `include_str!` rerun triggers so editing a translation re-bakes, exactly like editing a `.rsx` file.
pub fn catalog_files(package_root: &Path) -> Vec<PathBuf> {
    catalog_sources(package_root)
        .into_iter()
        .map(|(_, path)| path)
        .collect()
}

/// Every `(locale tag, file)` that feeds the catalog, sorted and de-duplicated. Collected from two places (either configurable in `telar.toml`, both on by default): the project-wide `locales/` directory (a `<tag>.toml` per language, or a `<tag>/` subdir of per-module files), and — scanned recursively under `src/` — every `i18n/<tag>.toml` co-located with a module (`src/modules/battery/i18n/en.toml`). A locale is its file stem; all files for a tag merge into one keyspace.
fn catalog_sources(package_root: &Path) -> Vec<(String, PathBuf)> {
    let cfg = read_i18n_config(package_root);
    let mut sources = Vec::new();

    if !cfg.root.is_empty() {
        discover_root_dir(&package_root.join(&cfg.root), &mut sources);
    }
    if !cfg.scan.is_empty() {
        for file in
            telar_project::collect_files_by_ext(&package_root.join("src"), &["toml"], &|_| true)
        {
            let in_scan_dir = file
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some(cfg.scan.as_str());
            if in_scan_dir && let Some(tag) = file.file_stem().and_then(|s| s.to_str()) {
                sources.push((tag.to_string(), file));
            }
        }
    }

    sources.sort();
    sources.dedup();
    sources
}

/// Discovers `<tag>.toml` (single file per locale) and `<tag>/*.toml` (a subdir of per-module files, merged) directly under `root`, appending each as `(tag, file)`.
fn discover_root_dir(root: &Path, sources: &mut Vec<(String, PathBuf)>) {
    let Ok(dir) = std::fs::read_dir(root) else {
        return;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            for file in telar_project::collect_files_by_ext(&path, &["toml"], &|_| true) {
                sources.push((name.to_string(), file));
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml")
            && let Some(tag) = path.file_stem().and_then(|s| s.to_str())
        {
            sources.push((tag.to_string(), path));
        }
    }
}

/// Parses the catalog for `package_root`. Returns `Ok(None)` when no translation files are found (i18n unused), and `Err` for a malformed catalog (bad TOML, non-string message, a key defined twice for one locale).
pub fn parse_catalog(package_root: &Path) -> Result<Option<CatalogModel>, String> {
    let sources = catalog_sources(package_root);
    if sources.is_empty() {
        return Ok(None);
    }

    let mut files: Vec<(String, String, String)> = Vec::new();
    for (tag, path) in &sources {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        files.push((tag.clone(), content, path.display().to_string()));
    }
    let borrowed: Vec<(&str, &str, &str)> = files
        .iter()
        .map(|(tag, content, label)| (tag.as_str(), content.as_str(), label.as_str()))
        .collect();

    let default = read_i18n_config(package_root).default;
    CatalogModel::from_sources(&borrowed, default.as_deref()).map(Some)
}

/// Serializes the catalog to a Rust source module. Types are referenced through the `telar::i18n` facade so the generated file compiles with the same `rsx` dependency every generated `.rsx` file has.
pub fn to_source(model: &CatalogModel) -> String {
    let mut s = String::new();
    s.push_str("pub static CATALOG: Catalog = Catalog {\n");
    s.push_str(&format!(
        "    locales: &[{}],\n",
        ser_str_list(&model.locales)
    ));
    s.push_str(&format!(
        "    default_locale: {},\n",
        ser_str(&model.default_locale)
    ));
    s.push_str("    entries: &[\n");
    for (key, per_locale) in &model.entries {
        s.push_str(&format!(
            "        Entry {{ key: {}, messages: &[",
            ser_str(key)
        ));
        for (locale, message) in per_locale {
            s.push_str(&format!(
                "({}, {}), ",
                ser_str(locale),
                ser_message(message)
            ));
        }
        s.push_str("] },\n");
    }
    s.push_str("    ],\n};\n");

    // Imported by what the body mentions: a catalogue with no interpolation and no plurals would otherwise warn on two unused names, in a file its author cannot edit.
    let mut names = vec!["Catalog", "Entry", "Message"];
    names.extend(
        ["Part", "PluralCategory"]
            .into_iter()
            .filter(|name| s.contains(&format!("{name}::"))),
    );
    format!("use telar::i18n::{{{}}};\n\n{s}", names.join(", "))
}

fn ser_message(message: &MessageModel) -> String {
    match message {
        MessageModel::Plain(text) => format!("Message::Plain({})", ser_str(text)),
        MessageModel::Format(parts) => {
            let inner: Vec<String> = parts
                .iter()
                .map(|p| match p {
                    PartModel::Lit(s) => format!("Part::Lit({})", ser_str(s)),
                    PartModel::Arg(s) => format!("Part::Arg({})", ser_str(s)),
                })
                .collect();
            format!("Message::Format(&[{}])", inner.join(", "))
        }
        MessageModel::Plural(branches) => {
            let inner: Vec<String> = branches
                .iter()
                .map(|(category, message)| {
                    format!(
                        "(PluralCategory::{}, {})",
                        ser_category(category),
                        ser_message(message)
                    )
                })
                .collect();
            format!("Message::Plural(&[{}])", inner.join(", "))
        }
    }
}

/// The Rust variant name for a category written in a catalog. Which *category* a name spells is [`PluralCategory::parse`]'s decision, shared with the runtime loader; only the spelling of the emitted variant is the baker's.
fn ser_category(name: &str) -> &'static str {
    match PluralCategory::parse(name).unwrap_or(PluralCategory::Other) {
        PluralCategory::Zero => "Zero",
        PluralCategory::One => "One",
        PluralCategory::Two => "Two",
        PluralCategory::Few => "Few",
        PluralCategory::Many => "Many",
        PluralCategory::Other => "Other",
    }
}

fn ser_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ser_str_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| ser_str(s))
        .collect::<Vec<_>>()
        .join(", ")
}
#[cfg(test)]
#[path = "catalog_test.rs"]
mod tests;

#[cfg(test)]
#[path = "catalog_plural_test.rs"]
mod plural_tests;
