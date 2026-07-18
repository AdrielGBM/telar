//! Build-time i18n catalog baker.
//!
//! Discovers translation files — a project-wide `locales/<tag>.toml` (or `locales/<tag>/*.toml`), and/or
//! per-module `src/**/i18n/<tag>.toml` co-located with each module, all configurable via `[rsx.i18n]` in
//! `rsx.toml` — parses each into a keyed set of messages, and serializes
//! the merged catalog to a Rust source string (`pub static CATALOG: rsx::i18n::Catalog = ..;`) of pure
//! `&'static` data — the same host-only "parse once, emit `&'static`" approach as the svg baker. The parsed
//! [`CatalogModel`] is also queryable so the `t!` macro and markup emitters can validate keys and arguments at
//! compile time.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The crate-root module the baked catalog is wired under, and the path every `t!`/markup call site references.
pub const I18N_MODULE: &str = "__rsx_i18n";
pub const I18N_CATALOG_PATH: &str = "crate::__rsx_i18n::CATALOG";

/// One piece of a message: literal text or a `{name}` placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum PartModel {
    Lit(String),
    Arg(String),
}

/// A parsed message: `Plain` when it has no placeholders, `Format` otherwise.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageModel {
    Plain(String),
    Format(Vec<PartModel>),
}

impl MessageModel {
    /// The placeholder names this message expects, in source order (with duplicates removed).
    pub fn arg_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let MessageModel::Format(parts) = self {
            for p in parts {
                if let PartModel::Arg(name) = p
                    && !names.contains(name)
                {
                    names.push(name.clone());
                }
            }
        }
        names
    }
}

/// The whole project's parsed catalog: available locales, the fallback locale, and every key's per-locale
/// messages. `BTreeMap`s keep output deterministic so unchanged catalogs don't retrigger recompilation.
#[derive(Debug, Clone)]
pub struct CatalogModel {
    pub locales: Vec<String>,
    pub default_locale: String,
    pub entries: BTreeMap<String, BTreeMap<String, MessageModel>>,
}

impl CatalogModel {
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// The placeholder names expected for `key`, taken from the default-locale message (falling back to any
    /// locale that defines the key). Used to validate `t!` arguments.
    pub fn arg_names(&self, key: &str) -> Option<Vec<String>> {
        let per_locale = self.entries.get(key)?;
        let msg = per_locale
            .get(&self.default_locale)
            .or_else(|| per_locale.values().next())?;
        Some(msg.arg_names())
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

/// How the catalog is discovered, from `[rsx.i18n]` in `rsx.toml` (with back-compat fallbacks to the older
/// `[rsx] locales` / `[rsx] default_locale`). Both discovery sources may be active at once; set either name to
/// `""` to disable it.
struct I18nConfig {
    /// Project-wide catalog directory joined onto the package root (default `"locales"`): holds `<tag>.toml` or
    /// `<tag>/<module>.toml`. `""` disables it.
    root: String,
    /// Directory name discovered recursively under `src/` for co-located, per-module catalogs (default
    /// `"i18n"`): every `src/**/<scan>/<tag>.toml` contributes to locale `<tag>`. `""` disables it.
    scan: String,
    /// Fallback locale when the active one lacks a key. `None` → `"en"` if present, else the first tag.
    default: Option<String>,
}

fn read_i18n_config(package_root: &Path) -> I18nConfig {
    let rsx = read_rsx_section(package_root);
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

/// The locale files that feed the catalog, sorted — used to emit `include_str!` rerun triggers so editing a
/// translation re-bakes, exactly like editing a `.rsx` file.
pub fn catalog_files(package_root: &Path) -> Vec<PathBuf> {
    catalog_sources(package_root)
        .into_iter()
        .map(|(_, path)| path)
        .collect()
}

/// Every `(locale tag, file)` that feeds the catalog, sorted and de-duplicated. Collected from two places
/// (either configurable in `rsx.toml`, both on by default): the project-wide `locales/` directory (a
/// `<tag>.toml` per language, or a `<tag>/` subdir of per-module files), and — scanned recursively under
/// `src/` — every `i18n/<tag>.toml` co-located with a module (`src/modules/battery/i18n/en.toml`). A locale is
/// its file stem; all files for a tag merge into one keyspace.
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

/// Discovers `<tag>.toml` (single file per locale) and `<tag>/*.toml` (a subdir of per-module files, merged)
/// directly under `root`, appending each as `(tag, file)`.
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

fn read_rsx_section(package_root: &Path) -> Option<toml::Table> {
    let content = std::fs::read_to_string(package_root.join("rsx.toml")).ok()?;
    content
        .parse::<toml::Table>()
        .ok()?
        .get("rsx")?
        .as_table()
        .cloned()
}

/// Parses the catalog for `package_root`. Returns `Ok(None)` when no translation files are found (i18n unused),
/// and `Err` for a malformed catalog (bad TOML, non-string message, a key defined twice for one locale).
pub fn parse_catalog(package_root: &Path) -> Result<Option<CatalogModel>, String> {
    let sources = catalog_sources(package_root);
    if sources.is_empty() {
        return Ok(None);
    }

    let mut entries: BTreeMap<String, BTreeMap<String, MessageModel>> = BTreeMap::new();
    let mut locales: Vec<String> = Vec::new();
    for (tag, path) in &sources {
        if !locales.contains(tag) {
            locales.push(tag.clone());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let table: toml::Table = content
            .parse()
            .map_err(|e| format!("parsing {}: {e}", path.display()))?;
        let mut flat = BTreeMap::new();
        flatten(&table, String::new(), &mut flat, path)?;
        for (key, message) in flat {
            let per_locale = entries.entry(key.clone()).or_default();
            if per_locale.insert(tag.clone(), message).is_some() {
                return Err(format!(
                    "{}: duplicate key `{key}` for locale `{tag}` (already defined in another file)",
                    path.display()
                ));
            }
        }
    }
    locales.sort();

    let default_locale = read_i18n_config(package_root)
        .default
        .filter(|d| locales.contains(d))
        .or_else(|| locales.iter().find(|l| *l == "en").cloned())
        .unwrap_or_else(|| locales[0].clone());

    Ok(Some(CatalogModel {
        locales,
        default_locale,
        entries,
    }))
}

/// Flattens nested TOML tables into dotted keys (`[settings] title = ".."` → `settings.title`). String scalars
/// become messages; other scalars are coerced to their display; arrays/datetimes are rejected.
fn flatten(
    table: &toml::Table,
    prefix: String,
    out: &mut BTreeMap<String, MessageModel>,
    path: &Path,
) -> Result<(), String> {
    for (key, value) in table {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(inner) => flatten(inner, full, out, path)?,
            toml::Value::String(s) => {
                out.insert(full, parse_message(s));
            }
            toml::Value::Integer(_) | toml::Value::Float(_) | toml::Value::Boolean(_) => {
                out.insert(full, MessageModel::Plain(value.to_string()));
            }
            other => {
                return Err(format!(
                    "{}: key `{full}` has unsupported type `{}` (translations must be strings)",
                    path.display(),
                    other.type_str()
                ));
            }
        }
    }
    Ok(())
}

/// Splits a message string into literal/placeholder parts. `{name}` is a named placeholder (whitespace inside
/// is trimmed); `{{` / `}}` are escaped literal braces. A string with no placeholders yields [`MessageModel::Plain`].
pub fn parse_message(content: &str) -> MessageModel {
    let mut parts: Vec<PartModel> = Vec::new();
    let mut literal = String::new();
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    parts.push(PartModel::Lit(std::mem::take(&mut literal)));
                }
                let mut name = String::new();
                for ec in chars.by_ref() {
                    if ec == '}' {
                        break;
                    }
                    name.push(ec);
                }
                parts.push(PartModel::Arg(name.trim().to_string()));
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        parts.push(PartModel::Lit(literal));
    }
    if parts.iter().all(|p| matches!(p, PartModel::Lit(_))) {
        let text: String = parts
            .into_iter()
            .map(|p| match p {
                PartModel::Lit(s) => s,
                PartModel::Arg(_) => unreachable!(),
            })
            .collect();
        MessageModel::Plain(text)
    } else {
        MessageModel::Format(parts)
    }
}

/// Serializes the catalog to a Rust source module. Types are referenced through the `rsx::i18n` facade so the
/// generated file compiles with the same `rsx` dependency every generated `.rsx` file has.
pub fn to_source(model: &CatalogModel) -> String {
    let mut s = String::new();
    s.push_str("use rsx::i18n::{Catalog, Entry, Message, Part};\n\n");
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
    s
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
    fn parses_plain_and_formatted_messages() {
        assert_eq!(
            parse_message("Settings"),
            MessageModel::Plain("Settings".into())
        );
        assert_eq!(
            parse_message("{time} remaining"),
            MessageModel::Format(vec![
                PartModel::Arg("time".into()),
                PartModel::Lit(" remaining".into()),
            ])
        );
        // Whitespace inside the braces is trimmed; escaped braces are literal.
        assert_eq!(
            parse_message("{{ {name} }}"),
            MessageModel::Format(vec![
                PartModel::Lit("{ ".into()),
                PartModel::Arg("name".into()),
                PartModel::Lit(" }".into()),
            ])
        );
    }

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
        // `en` is split across per-module files under `locales/en/`; `es` stays a single file.
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
        // Both the split `en` files and the single `es` file merged into one keyspace.
        assert!(catalog_files(&root).len() == 3);

        // A key defined twice for the same locale (across files) is a hard error.
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
        // Per-module catalogs co-located under `src/**/i18n/`, plus a project-wide `locales/` for shared keys.
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

        // `[rsx.i18n] scan = "translations"` renames the co-located dir; `i18n/` is then ignored. The es locale
        // now comes only from the project-wide root, so `default = "es"` still resolves.
        std::fs::write(root.join("locales/es.toml"), "[common]\non = \"Sí\"\n").unwrap();
        std::fs::write(
            root.join("rsx.toml"),
            "[rsx.i18n]\nscan = \"translations\"\ndefault = \"es\"\n",
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
