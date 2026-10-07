//! CLDR plural categories, the operands a rule reads from a count, and the rules that pick a category.
//!
//! The rules are a seam: [`set_plural_rules`] installs a [`PluralRules`] for the process, and [`plural_category`] asks it. With nothing installed, [`BuiltinPluralRules`] answers, so core translates plurals with no plugin and no data tables. The `telar-i18n` plugin installs the full CLDR rules from ICU4X.
//!
//! Only cardinal rules: a catalog's plural table selects a message by `count`, and the catalog format has no ordinal form for an ordinal rule to choose between.

use std::sync::RwLock;

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

/// A count as CLDR plural rules read it: the absolute value split into its integer digits and the fraction digits as written, so `1` and `1.0` stay different numbers (English says "1 item" but "1.0 items").
///
/// CLDR names these `i`, `f` and `v`; its other operands follow from them, `t` and `w` being `f` and `v` with trailing zeros dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluralOperands {
    /// The integer digits: CLDR's `i`.
    pub integer: u64,
    /// The fraction digits read as an integer, trailing zeros included: CLDR's `f`.
    pub fraction: u64,
    /// How many fraction digits were written, trailing zeros included: CLDR's `v`.
    pub fraction_digits: u32,
}

impl PluralOperands {
    /// Reads a decimal written the way a `count` argument is: an optional sign, digits, and optionally a `.` and more digits. `None` for anything else, or for more digits than a `u64` holds.
    pub fn parse(text: &str) -> Option<Self> {
        let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
        let (integer, fraction) = match unsigned.split_once('.') {
            Some((integer, fraction)) if !fraction.is_empty() => (integer, fraction),
            Some(_) => return None,
            None => (unsigned, ""),
        };
        let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if integer.is_empty() || !all_digits(integer) || !all_digits(fraction) {
            return None;
        }
        Some(Self {
            integer: integer.parse().ok()?,
            fraction: if fraction.is_empty() {
                0
            } else {
                fraction.parse().ok()?
            },
            fraction_digits: u32::try_from(fraction.len()).ok()?,
        })
    }

    /// Whether the count was written without fraction digits, which is when most languages' rules distinguish anything at all.
    pub fn is_integer(self) -> bool {
        self.fraction_digits == 0
    }
}

macro_rules! operands_from_signed {
    ($($t:ty),*) => {$(
        impl From<$t> for PluralOperands {
            fn from(count: $t) -> Self {
                Self { integer: count.unsigned_abs() as u64, ..Self::default() }
            }
        }
    )*};
}

macro_rules! operands_from_unsigned {
    ($($t:ty),*) => {$(
        impl From<$t> for PluralOperands {
            fn from(count: $t) -> Self {
                Self { integer: count as u64, ..Self::default() }
            }
        }
    )*};
}

operands_from_signed!(i8, i16, i32, i64, isize);
operands_from_unsigned!(u8, u16, u32, u64, usize);

/// The rules that pick a [`PluralCategory`] for a count in a locale, installed for the process with [`set_plural_rules`].
///
/// `locale` is the tag the catalog is being read in, as the application spelled it (`"pt-BR"`, `"ru_RU"`), and may be empty when a message is rendered with no locale at all. A provider answers [`PluralCategory::Other`] for a locale it cannot place rather than guessing, since every plural message has an `other` branch.
pub trait PluralRules: Send + Sync {
    fn category(&self, locale: &str, operands: PluralOperands) -> PluralCategory;
}

/// The rules core ships: a hand-written subset of CLDR covering the language families that differ structurally, and the English rule (one/other) for every other language — right for most of them, and wrong in a way that is visible rather than silent.
///
/// Integer counts only. A count with fraction digits picks [`PluralCategory::Other`], which is what CLDR says for English-like languages and not for every language; [`PluralRules`] from the `telar-i18n` plugin read fractions exactly.
pub struct BuiltinPluralRules;

impl PluralRules for BuiltinPluralRules {
    fn category(&self, locale: &str, operands: PluralOperands) -> PluralCategory {
        if !operands.is_integer() {
            return PluralCategory::Other;
        }
        let lang = locale
            .split(['-', '_'])
            .next()
            .unwrap_or(locale)
            .to_ascii_lowercase();
        let n = operands.integer;
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
            _ => {
                if n == 1 {
                    PluralCategory::One
                } else {
                    PluralCategory::Other
                }
            }
        }
    }
}

static INSTALLED: RwLock<Option<&'static dyn PluralRules>> = RwLock::new(None);

/// Installs the plural rules every plural message is selected with from here on, replacing [`BuiltinPluralRules`]. Process-wide and replaceable, like [`set_catalog`](crate::set_catalog).
///
/// The slot belongs to the copy of this crate that holds it. A **hot-reload dylib** links a copy of its own, apart from its host's, so rules the host installed from `main` never reach the dylib's widgets: install them from code that runs in each image, such as `telar::app!`'s setup block, which runs in the host and again on every load of the dylib. Rules a dylib handed into its host's copy would outlive the code they point at.
pub fn set_plural_rules(rules: &'static dyn PluralRules) {
    *INSTALLED.write().unwrap_or_else(|e| e.into_inner()) = Some(rules);
}

/// The category `operands` falls into for `locale`, by the installed [`PluralRules`], or by [`BuiltinPluralRules`] when none is installed.
pub fn plural_category(locale: &str, operands: impl Into<PluralOperands>) -> PluralCategory {
    let operands = operands.into();
    match *INSTALLED.read().unwrap_or_else(|e| e.into_inner()) {
        Some(rules) => rules.category(locale, operands),
        None => BuiltinPluralRules.category(locale, operands),
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
