//! Build-time i18n catalog baker.
//!
//! Discovers translation files — a project-wide `locales/<tag>.toml` (or `locales/<tag>/*.toml`), and/or per-module `src/**/i18n/<tag>.toml` co-located with each module, all configurable via `[telar.i18n]` in `telar.toml` — parses each into a keyed set of messages, and serializes the merged catalog to a Rust source string (`pub static CATALOG: telar::i18n::Catalog = ..;`) of pure `&'static` data — the same host-only "parse once, emit `&'static`" approach as the svg baker. The parsed [`CatalogModel`] is also queryable so the `t!` macro and markup emitters can validate keys and arguments at compile time.
//!
//! The model and the TOML grammar are `i18n-core`'s, shared with the runtime loader an app without any `.rsx` file uses. What is baker-only is here: discovery, and serialization to Rust source.

use std::path::{Path, PathBuf};

use i18n_core::PluralCategory;
pub use i18n_core::{CatalogModel, MessageModel, PartModel, parse_message};

/// The crate-root module the baked catalog is wired under, and the path every `t!`/markup call site references.
pub const I18N_MODULE: &str = "__rsx_i18n";
/// Where generated code reaches the baked catalogue.
pub const I18N_CATALOG_PATH: &str = "crate::__rsx_i18n::CATALOG";

/// How the catalog is discovered, from `[telar.i18n]` in `telar.toml` (with back-compat fallbacks to the older `[telar] locales` / `[telar] default_locale`). Both discovery sources may be active at once; set either name to `""` to disable it.
struct I18nConfig {
    /// Project-wide catalog directory joined onto the package root (default `"locales"`): holds `<tag>.toml` or `<tag>/<module>.toml`. `""` disables it.
    root: String,
    /// Directory name discovered recursively under `src/` for co-located, per-module catalogs (default `"i18n"`): every `src/**/<scan>/<tag>.toml` contributes to locale `<tag>`. `""` disables it.
    scan: String,
    /// Fallback locale when the active one lacks a key. `None` → `"en"` if present, else the first tag.
    default: Option<String>,
}

fn read_i18n_config(package_root: &Path) -> I18nConfig {
    let rsx = crate::discovery::read_rsx_section(package_root);
    let i18n = rsx
        .as_ref()
        .and_then(|t| t.get("i18n"))
        .and_then(|v| v.as_table());
    let sub = |key: &str| {
        i18n.and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let top = |key: &str| {
        rsx.as_ref()
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    I18nConfig {
        root: sub("root")
            .or_else(|| top("locales"))
            .unwrap_or_else(|| "locales".to_string()),
        scan: sub("scan").unwrap_or_else(|| "i18n".to_string()),
        default: sub("default").or_else(|| top("default_locale")),
    }
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
        for file in crate::collect_files_by_ext(&package_root.join("src"), "toml", &|_| true) {
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
            for file in crate::collect_files_by_ext(&path, "toml", &|_| true) {
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
mod tests {
    use super::*;

    #[test]
    fn bakes_catalog_from_disk() {
        let root = std::env::temp_dir().join(format!("rsx_i18n_bake_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("locales")).unwrap();
        std::fs::write(
            root.join("locales/en.toml"),
            "title = \"Settings\"\n[battery]\nremaining = \"{time} remaining\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("locales/es.toml"),
            "title = \"Ajustes\"\n[battery]\nremaining = \"quedan {time}\"\n",
        )
        .unwrap();

        let model = parse_catalog(&root).unwrap().unwrap();
        assert_eq!(model.locales, vec!["en", "es"]);
        assert_eq!(model.default_locale, "en");
        assert!(model.contains_key("battery.remaining"));
        assert_eq!(model.arg_names("battery.remaining").unwrap(), vec!["time"]);

        let src = to_source(&model);
        assert!(src.contains("default_locale: \"en\""));
        assert!(src.contains("Entry { key: \"battery.remaining\""));
        assert!(src.contains("Message::Plain(\"Ajustes\")"));
        assert!(src.contains("Part::Arg(\"time\")"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merges_per_module_files_in_a_locale_dir() {
        let root = std::env::temp_dir().join(format!("rsx_i18n_multi_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("locales/en")).unwrap();
        std::fs::write(
            root.join("locales/en/settings.toml"),
            "[settings]\ntitle = \"Settings\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("locales/en/battery.toml"),
            "[battery]\nfull = \"Full\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("locales/es.toml"),
            "[settings]\ntitle = \"Ajustes\"\n[battery]\nfull = \"Llena\"\n",
        )
        .unwrap();

        let model = parse_catalog(&root).unwrap().unwrap();
        assert_eq!(model.locales, vec!["en", "es"]);
        assert!(model.contains_key("settings.title"));
        assert!(model.contains_key("battery.full"));
        assert!(catalog_files(&root).len() == 3);

        std::fs::write(
            root.join("locales/en/dup.toml"),
            "[settings]\ntitle = \"Oops\"\n",
        )
        .unwrap();
        let err = parse_catalog(&root).unwrap_err();
        assert!(err.contains("duplicate key `settings.title`"), "{err}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn discovers_co_located_module_catalogs() {
        let root = std::env::temp_dir().join(format!("rsx_i18n_colo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/modules/battery/i18n")).unwrap();
        std::fs::create_dir_all(root.join("src/modules/settings/i18n")).unwrap();
        std::fs::create_dir_all(root.join("locales")).unwrap();
        std::fs::write(
            root.join("src/modules/battery/i18n/en.toml"),
            "[battery]\nfull = \"Full\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/modules/battery/i18n/es.toml"),
            "[battery]\nfull = \"Llena\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/modules/settings/i18n/en.toml"),
            "[settings]\ntitle = \"Settings\"\n",
        )
        .unwrap();
        std::fs::write(root.join("locales/en.toml"), "[common]\non = \"On\"\n").unwrap();

        let model = parse_catalog(&root).unwrap().unwrap();
        assert_eq!(model.locales, vec!["en", "es"]);
        assert!(model.contains_key("battery.full"));
        assert!(model.contains_key("settings.title"));
        assert!(
            model.contains_key("common.on"),
            "project-wide locales/ still merges alongside the scan"
        );

        std::fs::write(root.join("locales/es.toml"), "[common]\non = \"Sí\"\n").unwrap();
        std::fs::write(
            root.join("telar.toml"),
            "[telar.i18n]\nscan = \"translations\"\ndefault = \"es\"\n",
        )
        .unwrap();
        let model2 = parse_catalog(&root).unwrap().unwrap();
        assert!(
            !model2.contains_key("battery.full"),
            "the renamed scan dir no longer matches i18n/"
        );
        assert_eq!(model2.default_locale, "es");
        assert!(
            model2.contains_key("common.on"),
            "the project-wide root is unaffected by the scan name"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_locales_dir_is_none() {
        let root = std::env::temp_dir().join(format!("rsx_i18n_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(parse_catalog(&root).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod plural_tests {
    use std::collections::BTreeMap;

    use super::*;

    fn table(src: &str) -> toml::Table {
        src.parse().unwrap()
    }

    fn flat(src: &str) -> BTreeMap<String, MessageModel> {
        let mut out = BTreeMap::new();
        i18n_core::flatten(&table(src), String::new(), &mut out, "t.toml").unwrap();
        out
    }

    #[test]
    fn plural_serializes_with_its_categories() {
        let out = flat("[items]\none = \"item\"\nother = \"items\"\n");
        let src = ser_message(&out["items"]);
        assert!(src.starts_with("Message::Plural(&["), "{src}");
        assert!(src.contains("PluralCategory::One"), "{src}");
        assert!(src.contains("PluralCategory::Other"), "{src}");
    }

    #[test]
    fn the_plural_category_import_is_emitted_only_when_used() {
        let plain = CatalogModel {
            locales: vec!["en".into()],
            default_locale: "en".into(),
            entries: BTreeMap::from([(
                "a".to_string(),
                BTreeMap::from([("en".to_string(), MessageModel::Plain("x".into()))]),
            )]),
        };
        assert!(!to_source(&plain).contains("PluralCategory"));

        let mut with_plural = plain;
        with_plural.entries.insert(
            "items".to_string(),
            BTreeMap::from([(
                "en".to_string(),
                MessageModel::Plural(BTreeMap::from([(
                    "other".to_string(),
                    MessageModel::Plain("items".into()),
                )])),
            )]),
        );
        assert!(to_source(&with_plural).contains("PluralCategory"));
    }
}
