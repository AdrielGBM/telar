//! The writing direction, and the logical-to-physical edge mapping it decides.

/// The writing direction the layout resolves logical edges against.
///
/// Layout is authored in *logical* terms — start/end rather than left/right — and resolved to physical edges when a style is handed to the engine. One build therefore serves both directions: flipping [`Direction`] re-resolves the tree in place instead of rebuilding it, which is why the intent is kept alongside the resolved style rather than baked into it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// The direction conventionally written with `locale`'s script, keyed by language subtag: Arabic, Hebrew, Persian, Urdu and the other right-to-left languages, matched against the tag's primary subtag so `ar-EG` resolves like `ar`.
    pub fn for_locale(locale: &str) -> Self {
        let lang = locale
            .split(['-', '_'])
            .next()
            .unwrap_or(locale)
            .to_ascii_lowercase();
        const RTL: &[&str] = &[
            "ar", "arc", "ckb", "dv", "fa", "ha", "he", "khw", "ks", "ps", "sd", "ur", "uz", "yi",
        ];
        if RTL.contains(&lang.as_str()) {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtl_languages_are_recognised_with_and_without_a_region() {
        for tag in ["ar", "ar-EG", "he_IL", "fa", "ur-PK", "HE"] {
            assert_eq!(Direction::for_locale(tag), Direction::Rtl, "{tag}");
        }
    }

    #[test]
    fn everything_else_is_left_to_right() {
        for tag in ["en", "es-AR", "ja", "", "zz"] {
            assert_eq!(Direction::for_locale(tag), Direction::Ltr, "{tag}");
        }
    }
}
