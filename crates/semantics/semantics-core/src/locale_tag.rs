//! [`LocaleTag`]: a language, as a BCP 47 tag read once into its canonical form.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use icu_locale_core::LanguageIdentifier;

/// A language a text is written in or a reader asked for: a BCP 47 language tag, parsed once and kept in its canonical casing (`en-US`, `sr-Latn-RS`, `es-419`).
///
/// Two tags are the same language exactly when they are equal, so `en_us`, `EN-US` and `en-US` are one tag wherever it is compared: a catalog lookup, a negotiation, the locale an address carries. Displayed, it is the canonical spelling.
///
/// What it keeps is the language identifier: language, script, region and variants. Extensions and private use (`-u-ca-buddhist`, `-x-posix`) are read past and dropped, since nothing that chooses a language reads them.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LocaleTag(LanguageIdentifier);

impl LocaleTag {
    /// `tag` as a locale tag. Subtags may be separated by `-` or by the `_` of the POSIX spelling many platforms report, in any case.
    pub fn parse(tag: &str) -> Result<Self, LocaleTagError> {
        let hyphenated = tag.replace('_', "-");
        LanguageIdentifier::try_from_locale_bytes(hyphenated.as_bytes())
            .map(Self)
            .map_err(|_| LocaleTagError { tag: tag.into() })
    }

    #[doc(hidden)]
    pub const fn from_identifier(identifier: LanguageIdentifier) -> Self {
        Self(identifier)
    }

    /// The primary language subtag, lowercase: `sr` for `sr-Latn-RS`.
    pub fn language(&self) -> &str {
        self.0.language.as_str()
    }

    /// The script subtag in title case, where the tag names one: `Latn` for `sr-Latn-RS`.
    pub fn script(&self) -> Option<&str> {
        self.0.script.as_ref().map(|script| script.as_str())
    }

    /// The region subtag in upper case, where the tag names one: `RS` for `sr-Latn-RS`.
    pub fn region(&self) -> Option<&str> {
        self.0.region.as_ref().map(|region| region.as_str())
    }

    /// The variant subtags in canonical order, lowercase: `rozaj` and `biske` for `sl-IT-rozaj-biske`.
    pub fn variants(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.variants.iter().map(|variant| variant.as_str())
    }

    /// The identifier the Unicode libraries that apply a language's rules take.
    pub fn identifier(&self) -> &LanguageIdentifier {
        &self.0
    }
}

/// A [`LocaleTag`] written in source: checked and put in canonical form at compile time, so it can stand in a `static`. Constant evaluation holds at most one variant, so a tag with two or more is parsed at runtime with [`LocaleTag::parse`] instead.
///
/// ```
/// use telar_semantics_core::{LocaleTag, locale_tag};
///
/// static SPANISH: LocaleTag = locale_tag!("es-cl");
/// assert_eq!(SPANISH, LocaleTag::parse("es_CL").unwrap());
/// ```
#[macro_export]
macro_rules! locale_tag {
    ($tag:literal) => {
        $crate::LocaleTag::from_identifier($crate::__icu::langid!($tag))
    };
}

impl fmt::Display for LocaleTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for LocaleTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LocaleTag({})", self.0)
    }
}

impl FromStr for LocaleTag {
    type Err = LocaleTagError;

    fn from_str(tag: &str) -> Result<Self, LocaleTagError> {
        Self::parse(tag)
    }
}

/// Ordered as the canonical spellings are, so a sorted set of tags reads alphabetically.
impl Ord for LocaleTag {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.strict_cmp(other.to_string().as_bytes())
    }
}

impl PartialOrd for LocaleTag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Text that is not a BCP 47 language tag.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocaleTagError {
    tag: Box<str>,
}

impl LocaleTagError {
    /// The text that did not parse.
    pub fn tag(&self) -> &str {
        &self.tag
    }
}

impl fmt::Display for LocaleTagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a BCP 47 language tag (a language such as `en`, then optionally a script, a region and variants: `sr-Latn-RS`)",
            self.tag
        )
    }
}

impl std::error::Error for LocaleTagError {}

#[cfg(feature = "serde")]
impl serde::Serialize for LocaleTag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LocaleTag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let tag = String::deserialize(deserializer)?;
        Self::parse(&tag).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "locale_tag.test.rs"]
mod tests;
