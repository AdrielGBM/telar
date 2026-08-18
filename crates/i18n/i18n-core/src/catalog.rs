//! The parsed, owned form of a catalog, and the TOML grammar that produces it.
//!
//! Two things need this and they arrive from opposite directions. The transpiler's baker parses a project's
//! `locales/` at build time and serializes the result to `&'static` Rust — the fast path, no parser in the
//! binary. An application with no `.rsx` file in it has no transpiler to do that, and loads the same TOML at
//! runtime instead, leaking it to `&'static` so every lookup afterwards is the identical
//! [`crate::translate`] over the identical [`Catalog`].
//!
//! Both read the *same grammar*, which is why it lives here rather than in either of them: a `{name}`
//! placeholder, an escaped brace or a plural table that meant one thing to the baker and another at runtime
//! would be a bug nobody could see from either side.

use std::collections::BTreeMap;

use crate::message::{Catalog, Entry, Message, Part};
use crate::plural::PluralCategory;

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
    /// Per-CLDR-category messages, keyed by category name and always containing `other`. `BTreeMap` keeps the
    /// emitted order deterministic, as everywhere else in the baker.
    Plural(BTreeMap<String, MessageModel>),
}

impl MessageModel {
    /// The placeholder names this message expects, in source order (with duplicates removed). A plural
    /// contributes the union across its branches: which one a call renders is a runtime decision.
    pub fn arg_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut push = |name: &String| {
            if !names.contains(name) {
                names.push(name.clone());
            }
        };
        match self {
            MessageModel::Plain(_) => {}
            MessageModel::Format(parts) => {
                for p in parts {
                    if let PartModel::Arg(name) = p {
                        push(name);
                    }
                }
            }
            MessageModel::Plural(branches) => {
                for name in branches.values().flat_map(MessageModel::arg_names) {
                    push(&name);
                }
            }
        }
        names
    }
}

/// A whole catalog as parsed: available locales, the fallback locale, and every key's per-locale messages.
/// `BTreeMap`s keep output deterministic so unchanged catalogs don't retrigger recompilation — and, for the
/// runtime path, give [`leak`](Self::leak) the sorted order [`Catalog`]'s binary searches rely on.
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

    /// Merges every `(locale, TOML, label)` into one catalog. `label` names the source in error messages —
    /// a file path for the baker, a locale tag for a caller that parsed a string it holds in memory.
    ///
    /// `default` is the fallback locale when the active one lacks a key; an unknown or absent one resolves to
    /// `en` if the catalog has it, else the first tag alphabetically. Defining one key twice for one locale is
    /// an error rather than a last-writer-wins: the two definitions came from different files, and which of
    /// them a user sees would otherwise depend on directory order.
    pub fn from_sources(
        sources: &[(&str, &str, &str)],
        default: Option<&str>,
    ) -> Result<Self, String> {
        let mut entries: BTreeMap<String, BTreeMap<String, MessageModel>> = BTreeMap::new();
        let mut locales: Vec<String> = Vec::new();
        for (tag, content, label) in sources {
            if !locales.iter().any(|l| l == tag) {
                locales.push((*tag).to_string());
            }
            let table: toml::Table = content
                .parse()
                .map_err(|e| format!("parsing {label}: {e}"))?;
            let mut flat = BTreeMap::new();
            flatten(&table, String::new(), &mut flat, label)?;
            for (key, message) in flat {
                let per_locale = entries.entry(key.clone()).or_default();
                if per_locale.insert((*tag).to_string(), message).is_some() {
                    return Err(format!(
                        "{label}: duplicate key `{key}` for locale `{tag}` (already defined in another file)"
                    ));
                }
            }
        }
        if locales.is_empty() {
            return Err("a catalog needs at least one locale".to_string());
        }
        locales.sort();

        let default_locale = default
            .map(str::to_string)
            .filter(|d| locales.contains(d))
            .or_else(|| locales.iter().find(|l| *l == "en").cloned())
            .unwrap_or_else(|| locales[0].clone());

        Ok(Self {
            locales,
            default_locale,
            entries,
        })
    }

    /// The same catalog as `&'static` data, indistinguishable from a baked one at the point of use.
    ///
    /// It leaks, deliberately: [`Catalog`] is `&'static` throughout so that the baked path costs no
    /// allocation and no lifetime, and a runtime-loaded catalog buys into the same deal. A catalog is loaded
    /// once at startup and read for the life of the process, so the leak is the allocation — call it that
    /// many times, not once per language switch.
    ///
    /// The returned data is heap-allocated, so unlike a catalog baked into a hot-reload dylib it stays valid
    /// after that dylib is unloaded.
    pub fn leak(&self) -> &'static Catalog {
        let locales: Vec<&'static str> = self.locales.iter().map(|s| leak_str(s)).collect();
        let entries: Vec<Entry> = self
            .entries
            .iter()
            .map(|(key, per_locale)| Entry {
                key: leak_str(key),
                messages: leak_slice(
                    per_locale
                        .iter()
                        .map(|(locale, message)| (leak_str(locale), leak_message(message)))
                        .collect(),
                ),
            })
            .collect();
        Box::leak(Box::new(Catalog {
            locales: leak_slice(locales),
            default_locale: leak_str(&self.default_locale),
            entries: leak_slice(entries),
        }))
    }
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn leak_slice<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

fn leak_message(message: &MessageModel) -> Message {
    match message {
        MessageModel::Plain(text) => Message::Plain(leak_str(text)),
        MessageModel::Format(parts) => Message::Format(leak_slice(
            parts
                .iter()
                .map(|p| match p {
                    PartModel::Lit(s) => Part::Lit(leak_str(s)),
                    PartModel::Arg(s) => Part::Arg(leak_str(s)),
                })
                .collect(),
        )),
        MessageModel::Plural(branches) => Message::Plural(leak_slice(
            branches
                .iter()
                .filter_map(|(name, message)| {
                    Some((PluralCategory::parse(name)?, leak_message(message)))
                })
                .collect(),
        )),
    }
}

/// Whether `table` spells a plural set rather than a namespace: every key is a CLDR category *and* `other`
/// is among them.
///
/// Both halves matter. Requiring `other` is what CLDR requires of any language, and it keeps a namespace
/// that happens to hold a single `one = "…"` key from being swallowed; requiring every key to be a category
/// keeps a namespace with a stray `few` sibling from being misread.
pub fn is_plural_table(table: &toml::Table) -> bool {
    !table.is_empty()
        && table.contains_key("other")
        && table.keys().all(|k| PluralCategory::parse(k).is_some())
}

/// Flattens nested TOML tables into dotted keys (`[settings] title = ".."` → `settings.title`). String scalars
/// become messages; other scalars are coerced to their display; arrays/datetimes are rejected.
pub fn flatten(
    table: &toml::Table,
    prefix: String,
    out: &mut BTreeMap<String, MessageModel>,
    label: &str,
) -> Result<(), String> {
    for (key, value) in table {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(inner) if is_plural_table(inner) => {
                let mut branches = BTreeMap::new();
                for (category, message) in inner {
                    let toml::Value::String(s) = message else {
                        return Err(format!(
                            "{label}: plural `{full}.{category}` must be a string, found `{}`",
                            message.type_str()
                        ));
                    };
                    branches.insert(category.clone(), parse_message(s));
                }
                out.insert(full, MessageModel::Plural(branches));
            }
            toml::Value::Table(inner) => flatten(inner, full, out, label)?,
            toml::Value::String(s) => {
                out.insert(full, parse_message(s));
            }
            toml::Value::Integer(_) | toml::Value::Float(_) | toml::Value::Boolean(_) => {
                out.insert(full, MessageModel::Plain(value.to_string()));
            }
            other => {
                return Err(format!(
                    "{label}: key `{full}` has unsupported type `{}` (translations must be strings)",
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

impl Catalog {
    /// A catalog parsed from one locale's TOML, for an application whose translations are a string it already
    /// holds — an `include_str!`, a downloaded pack, a test.
    ///
    /// The format is the one the baker reads: dotted or nested keys, `{name}` placeholders, a table of CLDR
    /// categories for a plural.
    pub fn from_toml(locale: &str, toml: &str) -> Result<&'static Catalog, String> {
        Ok(CatalogModel::from_sources(&[(locale, toml, locale)], Some(locale))?.leak())
    }

    /// A catalog loaded from a directory of `<locale>.toml` files — the `locales/` layout the transpiler bakes
    /// from, read at runtime instead.
    ///
    /// This is the answer for an application with no `.rsx` files: the baked `CATALOG` only exists where the
    /// transpiler ran, so without this there is no way to reach [`crate::translate`] at all. `default` names
    /// the fallback locale; `None` resolves to `en` when present, else the first tag alphabetically.
    pub fn from_dir(
        dir: impl AsRef<std::path::Path>,
        default: Option<&str>,
    ) -> Result<&'static Catalog, String> {
        let dir = dir.as_ref();
        let read = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let mut files: Vec<(String, String, String)> = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            files.push((tag.to_string(), content, path.display().to_string()));
        }
        if files.is_empty() {
            return Err(format!("no <locale>.toml files in {}", dir.display()));
        }
        files.sort();
        let sources: Vec<(&str, &str, &str)> = files
            .iter()
            .map(|(tag, content, label)| (tag.as_str(), content.as_str(), label.as_str()))
            .collect();
        Ok(CatalogModel::from_sources(&sources, default)?.leak())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(src: &str) -> BTreeMap<String, MessageModel> {
        let mut out = BTreeMap::new();
        let table: toml::Table = src.parse().unwrap();
        flatten(&table, String::new(), &mut out, "t.toml").unwrap();
        out
    }

    #[test]
    fn a_category_table_becomes_one_plural_key() {
        let out = flat("[items]\none = \"{count} item\"\nother = \"{count} items\"\n");
        assert_eq!(out.len(), 1, "not two namespaced keys: {out:?}");
        let MessageModel::Plural(branches) = &out["items"] else {
            panic!("expected a plural, got {:?}", out["items"]);
        };
        assert_eq!(branches.len(), 2);
        assert!(branches.contains_key("one") && branches.contains_key("other"));
    }

    #[test]
    fn a_namespace_table_is_still_flattened() {
        let out = flat("[nav]\noverview = \"Overview\"\nsettings = \"Settings\"\n");
        assert_eq!(out.len(), 2);
        assert!(out.contains_key("nav.overview") && out.contains_key("nav.settings"));
    }

    #[test]
    fn a_namespace_is_not_swallowed_just_because_one_key_looks_like_a_category() {
        // `other` alone is a plausible namespace entry; without the all-keys test this would misparse.
        let out = flat("[status]\none = \"Single\"\nname = \"Status\"\n");
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out.contains_key("status.one"));

        // And a category table missing `other` is a namespace, since CLDR requires `other` of every language.
        let out = flat("[thing]\none = \"One\"\nfew = \"Few\"\n");
        assert!(out.contains_key("thing.one"), "{out:?}");
    }

    #[test]
    fn plural_arg_names_union_the_branches() {
        let out = flat("[items]\none = \"one item\"\nother = \"{count} items\"\n");
        assert_eq!(out["items"].arg_names(), vec!["count".to_string()]);
    }

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

    /// The point of the whole module: what a runtime load produces must be indistinguishable from what the
    /// baker emits, down to the binary searches `Catalog::message` does over both.
    #[test]
    fn a_leaked_catalog_answers_like_a_baked_one() {
        let catalog = CatalogModel::from_sources(
            &[
                (
                    "en",
                    "title = \"Settings\"\n[battery]\nremaining = \"{time} remaining\"\n",
                    "en",
                ),
                (
                    "es",
                    "title = \"Ajustes\"\n[battery]\nremaining = \"quedan {time}\"\n",
                    "es",
                ),
            ],
            None,
        )
        .unwrap()
        .leak();

        assert_eq!(catalog.locales, ["en", "es"]);
        assert_eq!(catalog.default_locale, "en");
        assert_eq!(
            catalog
                .message("battery.remaining", "es")
                .unwrap()
                .render(&[("time", "5m")]),
            "quedan 5m"
        );
        // A locale with no entry for the key falls back to the default locale, as the baked path does.
        assert_eq!(
            catalog.message("title", "fr").unwrap().render(&[]),
            "Settings"
        );
        assert!(catalog.message("absent", "en").is_none());
    }

    #[test]
    fn a_plural_table_survives_the_round_trip() {
        let catalog = CatalogModel::from_sources(
            &[(
                "en",
                "[items]\none = \"{count} item\"\nother = \"{count} items\"\n",
                "en",
            )],
            None,
        )
        .unwrap()
        .leak();
        let message = catalog.message("items", "en").unwrap();
        assert_eq!(
            message
                .select("en", &[("count", "1")])
                .render(&[("count", "1")]),
            "1 item"
        );
        assert_eq!(
            message
                .select("en", &[("count", "3")])
                .render(&[("count", "3")]),
            "3 items"
        );
    }

    #[test]
    fn one_key_defined_twice_for_one_locale_is_an_error() {
        let err = CatalogModel::from_sources(
            &[
                ("en", "title = \"A\"\n", "a.toml"),
                ("en", "title = \"B\"\n", "b.toml"),
            ],
            None,
        )
        .unwrap_err();
        assert!(err.contains("duplicate key `title`"), "{err}");
    }

    #[test]
    fn a_directory_of_locale_files_loads_as_one_catalog() {
        let dir =
            std::env::temp_dir().join(format!("telar_runtime_catalog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("en.toml"), "greeting = \"Hello, {name}!\"\n").unwrap();
        std::fs::write(dir.join("es.toml"), "greeting = \"¡Hola, {name}!\"\n").unwrap();

        let catalog = Catalog::from_dir(&dir, Some("es")).unwrap();
        assert_eq!(catalog.default_locale, "es");
        assert_eq!(
            catalog
                .message("greeting", "en")
                .unwrap()
                .render(&[("name", "Ada")]),
            "Hello, Ada!"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
