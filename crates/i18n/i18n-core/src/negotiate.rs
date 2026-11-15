//! Choosing which of the locales an application ships to show a user who prefers others.

use semantics_core::LocaleTag;

/// Picks the locale to show from those an application has, given the user's preferences in order.
///
/// Deliberately three rules and no more, so the same answer can be computed anywhere a copy of them has to run (a page choosing its language before any wasm loads, say). For each preferred tag, most preferred first:
///
/// 1. **Exact.** An available tag equal to it: `es-CL` picks `es-CL`.
/// 2. **Language.** Otherwise, the first available tag, in `available`'s order, with the same primary language subtag: `es-CL` picks `es`, and `es` picks `es-MX` when that is the only Spanish on offer.
/// 3. Otherwise, on to the next preferred tag.
///
/// `None` when no preferred tag matches anything, or there are none, for the caller to fall back on a locale of its choosing.
///
/// ```
/// # use telar_i18n_core::{locale_tag, negotiate_locale};
/// let shipped = [locale_tag!("en"), locale_tag!("es")];
/// assert_eq!(
///     negotiate_locale(&[locale_tag!("es-CL"), locale_tag!("en")], &shipped),
///     Some(&locale_tag!("es"))
/// );
/// assert_eq!(negotiate_locale(&[locale_tag!("fr")], &shipped), None);
/// ```
pub fn negotiate_locale<'a>(
    preferred: &[LocaleTag],
    available: &'a [LocaleTag],
) -> Option<&'a LocaleTag> {
    preferred.iter().find_map(|wanted| {
        available.iter().find(|tag| *tag == wanted).or_else(|| {
            available
                .iter()
                .find(|tag| tag.language() == wanted.language())
        })
    })
}

#[cfg(test)]
#[path = "negotiate.test.rs"]
mod tests;
