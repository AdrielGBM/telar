//! What the user asked every application on the system for: a colour scheme, less motion, more contrast and an ordered list of languages.
//!
//! Every field can be unknown. A platform that cannot answer says so with `None` (or an empty locale list) rather than guessing, so a consumer can tell "the user wants light" from "nobody said" and keep its own default for the second.

/// The light/dark preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// The user's system-wide presentation preferences, as one snapshot.
///
/// Delivered whole by [`Event::SystemPreferencesChanged`](crate::Event::SystemPreferencesChanged): once before a surface's first resume, and again whenever any field changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SystemPreferences {
    /// `None` when the platform reports no preference or cannot be asked.
    pub color_scheme: Option<ColorScheme>,
    /// `Some(true)` when the user asked for less motion; `None` when the platform cannot say.
    pub reduced_motion: Option<bool>,
    /// `Some(true)` when the user asked for more contrast; `None` when the platform cannot say.
    pub high_contrast: Option<bool>,
    /// BCP 47 tags, most preferred first. Empty when the platform reports none.
    pub locales: Vec<String>,
}

impl SystemPreferences {
    /// `Some(true)` for a dark preference, `None` when the scheme is unknown.
    pub fn prefers_dark(&self) -> Option<bool> {
        self.color_scheme.map(|scheme| scheme == ColorScheme::Dark)
    }
}

/// A POSIX locale name (`es_CL.UTF-8`, `sr_RS@latin`) as a BCP 47 tag (`es-CL`, `sr-Latn-RS`).
///
/// `None` for the empty name and for `C`/`POSIX`, which name no language at all.
pub fn posix_locale_to_bcp47(raw: &str) -> Option<String> {
    let (name, modifier) = match raw.split_once('@') {
        Some((name, modifier)) => (name, Some(modifier)),
        None => (raw, None),
    };
    let name = name.split('.').next().unwrap_or_default().trim();
    if name.is_empty() || name.eq_ignore_ascii_case("c") || name.eq_ignore_ascii_case("posix") {
        return None;
    }
    let (language, region) = match name.split_once('_') {
        Some((language, region)) => (language, Some(region)),
        None => (name, None),
    };
    if language.is_empty() || !language.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let script = match modifier {
        Some("latin") => Some("Latn"),
        Some("cyrillic") => Some("Cyrl"),
        Some("devanagari") => Some("Deva"),
        _ => None,
    };
    let mut tag = language.to_ascii_lowercase();
    if let Some(script) = script {
        tag.push('-');
        tag.push_str(script);
    }
    if let Some(region) = region.filter(|r| !r.is_empty()) {
        tag.push('-');
        tag.push_str(&region.to_ascii_uppercase());
    }
    Some(tag)
}

/// The preferred locales a POSIX environment declares, most preferred first, following gettext's precedence.
///
/// `LANGUAGE` is a colon-separated priority list and comes first, but gettext ignores it when the effective locale (`LC_ALL`, else `LC_MESSAGES`, else `LANG`) is `C`, and so does this. The effective locale follows it. Duplicates keep their first position.
pub fn locales_from_env(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let effective = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| var(name).filter(|value| !value.is_empty()));
    let Some(effective) = effective else {
        return Vec::new();
    };
    let Some(effective_tag) = posix_locale_to_bcp47(&effective) else {
        return Vec::new();
    };
    let mut locales: Vec<String> = Vec::new();
    let language = var("LANGUAGE").unwrap_or_default();
    for tag in language
        .split(':')
        .filter_map(posix_locale_to_bcp47)
        .chain(std::iter::once(effective_tag))
    {
        if !locales.contains(&tag) {
            locales.push(tag);
        }
    }
    locales
}

/// [`locales_from_env`] over this process's environment.
pub fn system_locales_from_env() -> Vec<String> {
    locales_from_env(|name| std::env::var(name).ok())
}

#[cfg(test)]
#[path = "system_preferences_test.rs"]
mod tests;
