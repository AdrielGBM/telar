use icu_locale_core::Locale;
use telar::i18n::{catalog, use_locale};

/// Reads a tag the way the catalog lookup spells it, `_` included, and answers the root locale for one ICU4X cannot parse, so formatting degrades to CLDR's neutral patterns rather than failing.
pub(crate) fn parse_locale(tag: &str) -> Locale {
    Locale::try_from_str(&tag.replace('_', "-")).unwrap_or(Locale::UNKNOWN)
}

/// The locale text is formatted in: the active one, read reactively, or with none set the installed catalog's default, as a translated string would fall back.
pub(crate) fn active_locale() -> Locale {
    let active = use_locale();
    match active
        .as_deref()
        .or_else(|| catalog().map(|c| c.default_locale))
    {
        Some(tag) => parse_locale(tag),
        None => Locale::UNKNOWN,
    }
}
