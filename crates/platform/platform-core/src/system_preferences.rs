//! What the user asked every application on the system for: a colour scheme, less motion, more contrast, an ordered list of languages and larger text.
//!
//! Every field but the text scale can be unknown. A platform that cannot answer says so with `None` (or an empty locale list) rather than guessing, so a consumer can tell "the user wants light" from "nobody said" and keep its own default for the second. The text scale has no such gap: an unset scale and one the platform cannot read both mean text at its designed size, so a platform says [`TextScale::UNSCALED`] for either.

use semantics_core::{LocaleTag, LocaleTagError};

/// The light/dark preference, written `light` or `dark` wherever it is spelled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "&'static str", try_from = "String")]
pub enum ColorScheme {
    Light,
    Dark,
}

impl ColorScheme {
    pub const ALL: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];

    /// The word the scheme is written as.
    pub fn as_str(self) -> &'static str {
        match self {
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
        }
    }
}

/// A word that names no [`ColorScheme`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown colour scheme `{0}`, expected `light` or `dark`")]
pub struct UnknownColorScheme(pub String);

impl std::str::FromStr for ColorScheme {
    type Err = UnknownColorScheme;

    fn from_str(word: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.as_str() == word)
            .ok_or_else(|| UnknownColorScheme(word.to_string()))
    }
}

impl TryFrom<String> for ColorScheme {
    type Error = UnknownColorScheme;

    fn try_from(word: String) -> Result<Self, Self::Error> {
        word.parse()
    }
}

impl From<ColorScheme> for &'static str {
    fn from(scheme: ColorScheme) -> Self {
        scheme.as_str()
    }
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
    /// Most preferred first. Empty when the platform reports none.
    pub locales: Vec<LocaleTag>,
    /// What the user asked every text to be multiplied by. A platform with no such setting, or one it cannot read, says [`TextScale::UNSCALED`].
    pub text_scale: TextScale,
}

/// How much larger than its design the user wants text drawn: `1.0` as designed, `2.0` twice the size. Always finite and above zero, so a reader never has to guard a scale that would make text vanish.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TextScale(f32);

impl TextScale {
    /// Text at the size it was designed at: what a platform says when the user set nothing or it cannot ask.
    pub const UNSCALED: TextScale = TextScale(1.0);

    /// `factor` as a text scale, or `None` when it is not a finite number above zero.
    pub fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(factor))
    }

    /// The factor every text size is multiplied by.
    pub fn get(self) -> f32 {
        self.0
    }
}

impl Default for TextScale {
    fn default() -> Self {
        Self::UNSCALED
    }
}

// The factor is finite and above zero by construction, so equality by value is reflexive and agrees with the bits hashed.
impl Eq for TextScale {}

impl std::hash::Hash for TextScale {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl SystemPreferences {
    /// `Some(true)` for a dark preference, `None` when the scheme is unknown.
    pub fn prefers_dark(&self) -> Option<bool> {
        self.color_scheme.map(|scheme| scheme == ColorScheme::Dark)
    }
}

/// A POSIX locale name (`es_CL.UTF-8`, `sr_RS@latin`) as a locale tag (`es-CL`, `sr-Latn-RS`).
///
/// `Ok(None)` for the empty name and for `C`/`POSIX`, which name no language at all, and an error for a name that is no locale.
pub fn posix_locale_to_bcp47(raw: &str) -> Result<Option<LocaleTag>, LocaleTagError> {
    let (name, modifier) = match raw.split_once('@') {
        Some((name, modifier)) => (name, Some(modifier)),
        None => (raw, None),
    };
    let name = name.split('.').next().unwrap_or_default().trim();
    if name.is_empty() || name.eq_ignore_ascii_case("c") || name.eq_ignore_ascii_case("posix") {
        return Ok(None);
    }
    let (language, region) = match name.split_once('_') {
        Some((language, region)) => (language, Some(region)),
        None => (name, None),
    };
    let script = match modifier {
        Some("latin") => Some("Latn"),
        Some("cyrillic") => Some("Cyrl"),
        Some("devanagari") => Some("Deva"),
        _ => None,
    };
    let mut tag = language.to_string();
    if let Some(script) = script {
        tag.push('-');
        tag.push_str(script);
    }
    if let Some(region) = region.filter(|r| !r.is_empty()) {
        tag.push('-');
        tag.push_str(region);
    }
    LocaleTag::parse(&tag).map(Some)
}

/// The preferred locales a POSIX environment declares, most preferred first, following gettext's precedence.
///
/// `LANGUAGE` is a colon-separated priority list and comes first, but gettext ignores it when the effective locale (`LC_ALL`, else `LC_MESSAGES`, else `LANG`) is `C`, and so does this. The effective locale follows it. Duplicates keep their first position. A name that is no locale is left out with a warning naming the variable and the name.
pub fn locales_from_env(var: impl Fn(&str) -> Option<String>) -> Vec<LocaleTag> {
    let Some((effective_var, effective)) =
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| {
                var(name)
                    .filter(|value| !value.is_empty())
                    .map(|value| (name, value))
            })
    else {
        return Vec::new();
    };
    let effective_tag = match posix_locale_to_bcp47(&effective) {
        Ok(None) => return Vec::new(),
        Ok(Some(tag)) => Some(tag),
        Err(error) => {
            warn_unread(effective_var, &effective, &error);
            None
        }
    };
    let language = var("LANGUAGE").unwrap_or_default();
    let listed = language
        .split(':')
        .filter_map(|name| match posix_locale_to_bcp47(name) {
            Ok(tag) => tag,
            Err(error) => {
                warn_unread("LANGUAGE", name, &error);
                None
            }
        });
    let mut locales: Vec<LocaleTag> = Vec::new();
    for tag in listed.chain(effective_tag) {
        if !locales.contains(&tag) {
            locales.push(tag);
        }
    }
    locales
}

fn warn_unread(var: &str, name: &str, error: &LocaleTagError) {
    tracing::warn!(
        "{var} names `{name}`, which is no locale, so it is not among the preferred ones: {error}"
    );
}

/// The locales a platform reported, most preferred first, as tags. One that is no BCP 47 tag is left out with a warning naming it, since a preference the app cannot read is one it cannot honour, and duplicates keep their first position.
pub fn platform_locales<S: AsRef<str>>(reported: impl IntoIterator<Item = S>) -> Vec<LocaleTag> {
    let mut locales: Vec<LocaleTag> = Vec::new();
    for reported in reported {
        match LocaleTag::parse(reported.as_ref()) {
            Ok(tag) if !locales.contains(&tag) => locales.push(tag),
            Ok(_) => {}
            Err(error) => tracing::warn!(
                "the platform prefers a locale that is no BCP 47 tag, so it is not among the preferred ones: {error}"
            ),
        }
    }
    locales
}

/// [`locales_from_env`] over this process's environment.
pub fn system_locales_from_env() -> Vec<LocaleTag> {
    locales_from_env(|name| std::env::var(name).ok())
}

#[cfg(test)]
#[path = "system_preferences.test.rs"]
mod tests;
