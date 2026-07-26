//! CLDR plural categories and the rules that pick one for a count.
//!
//! Hand-rolled rather than pulled from `icu`: the rules below are a *subset* of CLDR covering the language
//! families that differ structurally, and they are `const`-friendly pure functions with no data tables to
//! load. A locale that matches nothing falls back to the English rule (one/other), which is right for the
//! large majority of languages and wrong in a way that is visible rather than silent.

/// The CLDR plural categories. A catalog entry never has to define all six — only the ones its language
/// uses, plus `Other`, which CLDR requires as the fallback for every language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// Parses a category written in a catalog (`one`, `other`, …). `None` for anything else, which is what
    /// lets the baker tell a plural table apart from a namespace table.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "zero" => PluralCategory::Zero,
            "one" => PluralCategory::One,
            "two" => PluralCategory::Two,
            "few" => PluralCategory::Few,
            "many" => PluralCategory::Many,
            "other" => PluralCategory::Other,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }
}

/// The category `count` falls into for `locale`, keyed by the language subtag (so `pt-BR` resolves like
/// `pt`). Negative counts are treated by magnitude, matching CLDR's use of the absolute value.
pub fn plural_category(locale: &str, count: i64) -> PluralCategory {
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    let n = count.unsigned_abs();
    match lang.as_str() {
        // No grammatical plural at all: one form covers every count.
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" | "my" | "lo" | "km" => {
            PluralCategory::Other
        }
        // Zero groups with one.
        "fr" | "hy" | "ff" | "kab" => {
            if n <= 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" => east_slavic(n),
        "pl" => polish(n),
        "cs" | "sk" => {
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        "ar" => arabic(n),
        // The English rule, which also covers es/de/it/nl/pt/sv/da/no/fi/he/tr and most others.
        _ => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

fn east_slavic(n: u64) -> PluralCategory {
    let (mod10, mod100) = (n % 10, n % 100);
    if mod10 == 1 && mod100 != 11 {
        PluralCategory::One
    } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
        PluralCategory::Few
    } else {
        PluralCategory::Many
    }
}

fn polish(n: u64) -> PluralCategory {
    let (mod10, mod100) = (n % 10, n % 100);
    if n == 1 {
        PluralCategory::One
    } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
        PluralCategory::Few
    } else {
        PluralCategory::Many
    }
}

fn arabic(n: u64) -> PluralCategory {
    let mod100 = n % 100;
    match n {
        0 => PluralCategory::Zero,
        1 => PluralCategory::One,
        2 => PluralCategory::Two,
        _ if (3..=10).contains(&mod100) => PluralCategory::Few,
        _ if (11..=99).contains(&mod100) => PluralCategory::Many,
        _ => PluralCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::PluralCategory::*;
    use super::*;

    #[test]
    fn english_splits_one_from_the_rest() {
        assert_eq!(plural_category("en", 0), Other);
        assert_eq!(plural_category("en", 1), One);
        assert_eq!(plural_category("en", 2), Other);
        // An unknown language falls back to this same rule.
        assert_eq!(plural_category("zz", 1), One);
    }

    #[test]
    fn a_region_subtag_resolves_like_its_language() {
        assert_eq!(plural_category("pt-BR", 2), Other);
        assert_eq!(plural_category("ru_RU", 2), Few);
    }

    #[test]
    fn languages_without_a_plural_always_pick_other() {
        for n in [0, 1, 2, 11, 100] {
            assert_eq!(plural_category("ja", n), Other, "{n}");
        }
    }

    #[test]
    fn french_groups_zero_with_one() {
        assert_eq!(plural_category("fr", 0), One);
        assert_eq!(plural_category("fr", 1), One);
        assert_eq!(plural_category("fr", 2), Other);
    }

    #[test]
    fn russian_uses_one_few_many() {
        for (n, want) in [
            (1, One),
            (21, One),
            (11, Many),
            (2, Few),
            (24, Few),
            (12, Many),
            (5, Many),
            (100, Many),
        ] {
            assert_eq!(plural_category("ru", n), want, "{n}");
        }
    }

    #[test]
    fn polish_keeps_one_for_exactly_one() {
        // Unlike Russian, 21 is not `one` in Polish.
        assert_eq!(plural_category("pl", 1), One);
        assert_eq!(plural_category("pl", 21), Many);
        assert_eq!(plural_category("pl", 22), Few);
    }

    #[test]
    fn arabic_uses_all_six() {
        for (n, want) in [
            (0, Zero),
            (1, One),
            (2, Two),
            (3, Few),
            (11, Many),
            (100, Other),
        ] {
            assert_eq!(plural_category("ar", n), want, "{n}");
        }
    }

    #[test]
    fn a_negative_count_is_read_by_magnitude() {
        assert_eq!(plural_category("en", -1), One);
        assert_eq!(plural_category("ru", -2), Few);
    }

    #[test]
    fn categories_round_trip_through_their_names() {
        for c in [Zero, One, Two, Few, Many, Other] {
            assert_eq!(PluralCategory::parse(c.as_str()), Some(c));
        }
        assert_eq!(PluralCategory::parse("nav"), None);
    }
}
