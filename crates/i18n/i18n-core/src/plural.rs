//! CLDR plural categories and the rules that pick one for a count.
//!
//! Hand-rolled rather than pulled from `icu`: the rules below are a *subset* of CLDR covering the language families that differ structurally, and they are `const`-friendly pure functions with no data tables to load. A locale that matches nothing falls back to the English rule (one/other), which is right for the large majority of languages and wrong in a way that is visible rather than silent.

/// The CLDR plural categories. A catalog entry never has to define all six — only the ones its language uses, plus `Other`, which CLDR requires as the fallback for every language.
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
    /// Parses a category written in a catalog (`one`, `other`, …). `None` for anything else, which is what lets the baker tell a plural table apart from a namespace table.
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

/// The category `count` falls into for `locale`, keyed by the language subtag (so `pt-BR` resolves like `pt`). Negative counts are treated by magnitude, matching CLDR's use of the absolute value.
pub fn plural_category(locale: &str, count: i64) -> PluralCategory {
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    let n = count.unsigned_abs();
    match lang.as_str() {
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" | "my" | "lo" | "km" => {
            PluralCategory::Other
        }
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
#[path = "plural_test.rs"]
mod tests;
