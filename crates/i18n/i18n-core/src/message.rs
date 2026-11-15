//! The baked message model. Every type here is `const`-constructible so the build-time catalog baker can emit a whole catalog as a single `static CATALOG: Catalog = Catalog { .. };` — pure `&'static` data with no runtime parsing and no heap, mirroring how the svg baker emits `&'static` draw commands.

use semantics_core::LocaleTag;

use crate::plural::{PluralCategory, PluralOperands, plural_category};

/// One piece of a message: literal text, or a named placeholder to be filled from the call's arguments.
pub enum Part {
    Lit(&'static str),
    Arg(&'static str),
}

/// A single translated string. `Plain` is the common placeholder-free case; `Format` carries the ordered literal/placeholder parts so parameters substitute by *name*, letting a translation reorder them freely relative to the source language.
pub enum Message {
    Plain(&'static str),
    Format(&'static [Part]),
    /// One message per plural category. The baker guarantees [`PluralCategory::Other`] is present, so selection always has a branch to land on however exotic the active locale's rules are.
    Plural(&'static [(PluralCategory, Message)]),
}

impl Message {
    /// Resolves a plural to the branch the installed [`PluralRules`](crate::PluralRules) select for `locale` and the `count` argument; a non-plural message is returned unchanged. A `count` that is not a decimal number selects as zero.
    ///
    /// Selection lives here rather than in [`render`](Self::render) because it needs the active locale — which category a count falls into is a property of the *language*, not of the message.
    pub fn select(&self, locale: &LocaleTag, args: &[(&str, &str)]) -> &Message {
        let Message::Plural(branches) = self else {
            return self;
        };
        let count = args
            .iter()
            .find(|(name, _)| *name == "count")
            .and_then(|(_, value)| PluralOperands::parse(value))
            .unwrap_or_default();
        let wanted = plural_category(locale, count);
        branch(branches, wanted)
            .or_else(|| branch(branches, PluralCategory::Other))
            .unwrap_or(self)
    }

    /// Renders the message, substituting each `Arg(name)` with the matching value from `args`. An argument with no matching placeholder is ignored; a placeholder with no matching argument is left visible as `{name}` so the gap surfaces instead of silently vanishing.
    pub fn render(&self, args: &[(&str, &str)]) -> String {
        match self {
            // Only the direct-call path: `translate` selects first. With no locale to choose with, `Other` is the honest fallback.
            Message::Plural(branches) => branch(branches, PluralCategory::Other)
                .map_or_else(String::new, |other| other.render(args)),
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

    /// The placeholder names this message expects, in source order. Used by the build-time validator to diagnose a `t!` call whose arguments don't match the message's placeholders.
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

fn branch(branches: &[(PluralCategory, Message)], category: PluralCategory) -> Option<&Message> {
    branches
        .iter()
        .find(|(branch, _)| *branch == category)
        .map(|(_, message)| message)
}

/// One key's translations: the message for each locale that has one, at most one per locale.
pub struct Entry {
    pub key: &'static str,
    pub messages: &'static [(LocaleTag, Message)],
}

/// A project's baked catalog: every translatable key with its per-locale messages, the set of available locales, and the fallback locale used when the active locale lacks a given key.
///
/// The whole value is immutable `&'static` data, so it is `Sync` and every `t!`/markup call site references it by path — there is no global install step and nothing outlives the dylib it was baked into, keeping hot reload safe (the same reason the svg baker references its `BAKED_SVG_N` statics by path).
pub struct Catalog {
    pub locales: &'static [LocaleTag],
    pub default_locale: LocaleTag,
    /// Sorted by `key` so `message` can binary-search.
    pub entries: &'static [Entry],
}

impl Catalog {
    /// Resolves `key` for `locale`, falling back to [`Catalog::default_locale`] when the key exists but has no message for the active locale. `None` only when the key is absent entirely.
    pub fn message(&self, key: &str, locale: &LocaleTag) -> Option<&Message> {
        self.message_in(key, locale)
            .or_else(|| self.message_in(key, &self.default_locale))
    }

    pub(crate) fn message_in(&self, key: &str, locale: &LocaleTag) -> Option<&Message> {
        self.entries
            .binary_search_by(|e| e.key.cmp(key))
            .ok()
            .and_then(|i| self.entries[i].lookup(locale))
    }

    /// Whether `key` exists in the catalog (in any locale). Used by the build-time `t!` validator.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.binary_search_by(|e| e.key.cmp(key)).is_ok()
    }

    /// Panics naming the first key that is out of order or repeated, or the first locale a key has two messages for.
    ///
    /// Keys are binary-searched, so an entry out of order is silently never found, and only the first of two messages for one locale is ever read. A baked or loaded catalog is sorted by construction; a hand-written `static` one calls this from a test.
    #[track_caller]
    pub fn assert_sorted(&self) {
        for pair in self.entries.windows(2) {
            assert!(
                pair[0].key < pair[1].key,
                "catalog keys `{}` and `{}` are out of order or repeated",
                pair[0].key,
                pair[1].key
            );
        }
        for entry in self.entries {
            for (index, (locale, _)) in entry.messages.iter().enumerate() {
                assert!(
                    !entry.messages[..index]
                        .iter()
                        .any(|(seen, _)| seen == locale),
                    "the locale `{locale}` of `{}` is repeated",
                    entry.key
                );
            }
        }
    }
}

impl Entry {
    fn lookup(&self, locale: &LocaleTag) -> Option<&Message> {
        self.messages
            .iter()
            .find(|(tag, _)| tag == locale)
            .map(|(_, message)| message)
    }
}

#[cfg(test)]
#[path = "message.test.rs"]
mod tests;
