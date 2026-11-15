use icu_locale_core::Locale;
use telar::LocaleTag;
use telar::i18n::{default_locale, use_locale};

/// The locale `tag` names, as the formatters and the plural rules take it.
pub(crate) fn icu_locale(tag: &LocaleTag) -> Locale {
    Locale::from(tag.identifier().clone())
}

/// The locale text is formatted in: the active one, read reactively, or with none set the installed catalog's default, as a translated string would fall back.
pub(crate) fn active_locale() -> Locale {
    let active = use_locale();
    icu_locale(active.as_ref().unwrap_or_else(|| default_locale()))
}
