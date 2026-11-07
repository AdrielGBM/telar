//! Choosing which of the locales an application ships to show a user who prefers others.

use crate::locale_tag::{LocaleTag, same_tag};

/// Picks the locale to show from those an application has, given the user's preferences in order.
///
/// Deliberately three rules and no more, so the same answer can be computed anywhere a copy of them has to run (a page choosing its language before any wasm loads, say). For each preferred tag, most preferred first:
///
/// 1. **Exact.** An available tag equal to it, ignoring ASCII case and whether subtags are separated by `-` or `_`: `es-CL` picks `es-CL`, and `pt_BR` picks `pt-BR`.
/// 2. **Language.** Otherwise, the first available tag, in `available`'s order, with the same primary language subtag: `es-CL` picks `es`, and `es` picks `es-MX` when that is the only Spanish on offer.
/// 3. Otherwise, on to the next preferred tag.
///
/// When no preferred tag matches anything, or there are none, the answer is `fallback`. A match returns the tag as `available` spells it.
///
/// ```
/// # use telar_i18n_core::negotiate_locale;
/// assert_eq!(negotiate_locale(&["es-CL", "en"], &["en", "es"], "en"), "es");
/// assert_eq!(negotiate_locale(&["fr"], &["en", "es"], "es"), "es");
/// ```
pub fn negotiate_locale<'a>(
    preferred: &[impl AsRef<str>],
    available: &'a [impl AsRef<str>],
    fallback: &'a str,
) -> &'a str {
    for wanted in preferred {
        let wanted = wanted.as_ref();
        if let Some(exact) = available
            .iter()
            .map(AsRef::as_ref)
            .find(|tag| same_tag(tag, wanted))
        {
            return exact;
        }
        let language = LocaleTag::parse(wanted).language;
        if language.is_empty() {
            continue;
        }
        if let Some(same_language) = available.iter().map(AsRef::as_ref).find(|tag| {
            LocaleTag::parse(tag)
                .language
                .eq_ignore_ascii_case(language)
        }) {
            return same_language;
        }
    }
    fallback
}

#[cfg(test)]
#[path = "negotiate.test.rs"]
mod tests;
