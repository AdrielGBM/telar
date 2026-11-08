//! The writing direction, and which one a locale is written in.

use crate::locale_tag::LocaleTag;

/// The direction text is written in, which layout resolves logical edges against.
///
/// Layout is authored in *logical* terms — start/end rather than left/right — and resolved to physical edges when a style is handed to the engine. One build therefore serves both directions: flipping [`Direction`] re-resolves the tree in place instead of rebuilding it, which is why the intent is kept alongside the resolved style rather than baked into it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// ISO 15924 codes of the scripts Unicode writes right to left.
const RTL_SCRIPTS: &[&str] = &[
    "Adlm", "Arab", "Aran", "Armi", "Avst", "Chrs", "Cprt", "Elym", "Gara", "Hatr", "Hebr", "Hung",
    "Khar", "Lydi", "Mand", "Mani", "Mend", "Merc", "Mero", "Narb", "Nbat", "Nkoo", "Orkh", "Ougr",
    "Palm", "Phli", "Phlp", "Phnx", "Prti", "Rohg", "Samr", "Sarb", "Sogd", "Sogo", "Syrc", "Syre",
    "Syrj", "Syrn", "Thaa", "Yezi",
];

/// Languages whose usual script is right to left. A language commonly written in more than one script (Hausa, Uzbek, Azerbaijani, Punjabi) is left out: its default is left to right, and a tag such as `uz-Arab` says otherwise.
const RTL_LANGUAGES: &[&str] = &[
    "ar", "arc", "ckb", "dv", "fa", "he", "khw", "ks", "ps", "sd", "ur", "yi",
];

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// The direction `locale` is written in. A script subtag decides when the tag has one, so `uz-Arab` is right to left and `sr-Cyrl` left to right; otherwise the language's usual script does, so `ar-EG` and `he_IL` are right to left and `uz` is not.
    pub fn for_locale(locale: &str) -> Self {
        let tag = LocaleTag::parse(locale);
        let rtl = match tag.script {
            Some(script) => RTL_SCRIPTS
                .iter()
                .any(|rtl| rtl.eq_ignore_ascii_case(script)),
            None => RTL_LANGUAGES
                .iter()
                .any(|rtl| rtl.eq_ignore_ascii_case(tag.language)),
        };
        if rtl { Direction::Rtl } else { Direction::Ltr }
    }
}

#[cfg(test)]
#[path = "direction.test.rs"]
mod tests;
