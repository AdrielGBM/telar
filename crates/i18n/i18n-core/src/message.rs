//! The baked message model. Every type here is `const`-constructible so the build-time catalog baker can
//! emit a whole catalog as a single `static CATALOG: Catalog = Catalog { .. };` — pure `&'static` data with
//! no runtime parsing and no heap, mirroring how the svg baker emits `&'static` draw commands.

use crate::plural::{PluralCategory, plural_category};

/// One piece of a message: literal text, or a named placeholder to be filled from the call's arguments.
pub enum Part {
    Lit(&'static str),
    Arg(&'static str),
}

/// A single translated string. `Plain` is the common placeholder-free case; `Format` carries the ordered
/// literal/placeholder parts so parameters substitute by *name*, letting a translation reorder them freely
/// relative to the source language.
pub enum Message {
    Plain(&'static str),
    Format(&'static [Part]),
    /// One message per plural category. The baker guarantees [`PluralCategory::Other`] is present, so
    /// selection always has a branch to land on however exotic the active locale's rules are.
    Plural(&'static [(PluralCategory, Message)]),
}

impl Message {
    /// Resolves a plural to the branch `locale` selects for the `count` argument; a non-plural message is
    /// returned unchanged.
    ///
    /// Selection lives here rather than in [`render`](Self::render) because it needs the active locale —
    /// which category a count falls into is a property of the *language*, not of the message.
    pub fn select(&self, locale: &str, args: &[(&str, &str)]) -> &Message {
        let Message::Plural(branches) = self else {
            return self;
        };
        let count = args
            .iter()
            .find(|(name, _)| *name == "count")
            .and_then(|(_, value)| value.parse::<i64>().ok())
            .unwrap_or(0);
        let wanted = plural_category(locale, count);
        let pick = |c: PluralCategory| branches.iter().find(|(cat, _)| *cat == c).map(|(_, m)| m);
        pick(wanted)
            .or_else(|| pick(PluralCategory::Other))
            .unwrap_or(self)
    }

    /// Renders the message, substituting each `Arg(name)` with the matching value from `args`. An argument
    /// with no matching placeholder is ignored; a placeholder with no matching argument is left visible as
    /// `{name}` so the gap surfaces instead of silently vanishing.
    pub fn render(&self, args: &[(&str, &str)]) -> String {
        match self {
            // Only the direct-call path: `translate` selects first. With no locale to choose with, `Other` is the honest fallback.
            Message::Plural(_) => self.select("", args).render(args),
            Message::Plain(s) => (*s).to_string(),
            Message::Format(parts) => {
                let mut out = String::new();
                for part in *parts {
                    match part {
                        Part::Lit(s) => out.push_str(s),
                        Part::Arg(name) => match args.iter().find(|(n, _)| n == name) {
                            Some((_, value)) => out.push_str(value),
                            None => {
                                out.push('{');
                                out.push_str(name);
                                out.push('}');
                            }
                        },
                    }
                }
                out
            }
        }
    }

    /// The placeholder names this message expects, in source order. Used by the build-time validator to
    /// diagnose a `t!` call whose arguments don't match the message's placeholders.
    pub fn arg_names(&self) -> Vec<&'static str> {
        match self {
            Message::Plain(_) => Vec::new(),
            // The union across branches: a `t!` call must satisfy whichever branch the locale picks, and that is not known until runtime.
            Message::Plural(branches) => {
                let mut names = Vec::new();
                for (_, message) in *branches {
                    for name in message.arg_names() {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                names
            }
            Message::Format(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    Part::Arg(name) => Some(*name),
                    Part::Lit(_) => None,
                })
                .collect(),
        }
    }
}

/// One key's translations: the message for every available locale. `messages` is kept sorted by locale so
/// lookup can binary-search it.
pub struct Entry {
    pub key: &'static str,
    pub messages: &'static [(&'static str, Message)],
}

/// A project's baked catalog: every translatable key with its per-locale messages, the set of available
/// locales, and the fallback locale used when the active locale lacks a given key.
///
/// The whole value is immutable `&'static` data, so it is `Sync` and every `t!`/markup call site references
/// it by path — there is no global install step and nothing outlives the dylib it was baked into, keeping
/// hot reload safe (the same reason the svg baker references its `BAKED_SVG_N` statics by path).
pub struct Catalog {
    pub locales: &'static [&'static str],
    pub default_locale: &'static str,
    /// Sorted by `key` so `message` can binary-search.
    pub entries: &'static [Entry],
}

impl Catalog {
    /// Resolves `key` for `locale`, falling back to [`Catalog::default_locale`] when the key exists but has no
    /// message for the active locale. `None` only when the key is absent entirely.
    pub fn message(&self, key: &str, locale: &str) -> Option<&Message> {
        let entry = self
            .entries
            .binary_search_by(|e| e.key.cmp(key))
            .ok()
            .map(|i| &self.entries[i])?;
        entry
            .lookup(locale)
            .or_else(|| entry.lookup(self.default_locale))
    }

    /// Whether `key` exists in the catalog (in any locale). Used by the build-time `t!` validator.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.binary_search_by(|e| e.key.cmp(key)).is_ok()
    }
}

impl Entry {
    fn lookup(&self, locale: &str) -> Option<&Message> {
        self.messages
            .binary_search_by(|(l, _)| (*l).cmp(locale))
            .ok()
            .map(|i| &self.messages[i].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static CATALOG: Catalog = Catalog {
        locales: &["en", "es"],
        default_locale: "en",
        entries: &[
            Entry {
                key: "battery.remaining",
                messages: &[
                    (
                        "en",
                        Message::Format(&[Part::Arg("time"), Part::Lit(" remaining")]),
                    ),
                    (
                        "es",
                        Message::Format(&[Part::Lit("quedan "), Part::Arg("time")]),
                    ),
                ],
            },
            Entry {
                key: "settings.title",
                messages: &[
                    ("en", Message::Plain("Settings")),
                    ("es", Message::Plain("Ajustes")),
                ],
            },
        ],
    };

    #[test]
    fn plain_lookup_and_fallback() {
        assert_eq!(
            CATALOG.message("settings.title", "es").unwrap().render(&[]),
            "Ajustes"
        );
        // A locale with no entry for the key falls back to the default locale.
        assert_eq!(
            CATALOG.message("settings.title", "fr").unwrap().render(&[]),
            "Settings"
        );
        assert!(CATALOG.message("missing.key", "en").is_none());
    }

    #[test]
    fn named_args_substitute_and_reorder() {
        let en = CATALOG.message("battery.remaining", "en").unwrap();
        let es = CATALOG.message("battery.remaining", "es").unwrap();
        assert_eq!(en.render(&[("time", "5m")]), "5m remaining");
        // Spanish reorders the placeholder relative to the literal — named substitution handles it.
        assert_eq!(es.render(&[("time", "5m")]), "quedan 5m");
    }

    #[test]
    fn missing_arg_stays_visible() {
        let en = CATALOG.message("battery.remaining", "en").unwrap();
        assert_eq!(en.render(&[]), "{time} remaining");
    }

    #[test]
    fn arg_names_lists_placeholders() {
        assert_eq!(
            CATALOG
                .message("battery.remaining", "en")
                .unwrap()
                .arg_names(),
            vec!["time"]
        );
        assert!(
            CATALOG
                .message("settings.title", "en")
                .unwrap()
                .arg_names()
                .is_empty()
        );
    }
}
