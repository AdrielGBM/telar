use fixed_decimal::Decimal;
use icu_plurals::PluralRulesPreferences;
use telar::i18n::{PluralCategory, PluralOperands, PluralRules, set_plural_rules};

use crate::locale::parse_locale;

/// The CLDR cardinal plural rules for every locale ICU4X has data for, as a [`PluralRules`] for the catalog lookup.
///
/// A locale ICU4X cannot place resolves through its parent locales to the root locale, whose only category is [`PluralCategory::Other`].
pub struct CldrPluralRules;

impl PluralRules for CldrPluralRules {
    fn category(&self, locale: &str, operands: PluralOperands) -> PluralCategory {
        let prefs = PluralRulesPreferences::from(&parse_locale(locale));
        let Ok(rules) = icu_plurals::PluralRules::try_new_cardinal(prefs) else {
            return PluralCategory::Other;
        };
        let Ok(operands) = icu_operands(operands) else {
            return PluralCategory::Other;
        };
        match rules.category_for(operands) {
            icu_plurals::PluralCategory::Zero => PluralCategory::Zero,
            icu_plurals::PluralCategory::One => PluralCategory::One,
            icu_plurals::PluralCategory::Two => PluralCategory::Two,
            icu_plurals::PluralCategory::Few => PluralCategory::Few,
            icu_plurals::PluralCategory::Many => PluralCategory::Many,
            icu_plurals::PluralCategory::Other => PluralCategory::Other,
        }
    }
}

/// ICU4X builds its operands only from a number, so the count goes through a decimal spelled with the fraction digits exactly as they were written.
fn icu_operands(
    operands: PluralOperands,
) -> Result<icu_plurals::PluralOperands, fixed_decimal::ParseError> {
    let PluralOperands {
        integer,
        fraction,
        fraction_digits,
    } = operands;
    if fraction_digits == 0 {
        return Ok(integer.into());
    }
    let width = fraction_digits as usize;
    let decimal = Decimal::try_from_str(&format!("{integer}.{fraction:0width$}"))?;
    Ok((&decimal).into())
}

/// Selects every catalog plural from here on by [`CldrPluralRules`] instead of the table built into `telar`.
///
/// Process-wide, and it belongs to the copy of `telar` in the image that calls it: call it from `telar::app!`'s setup block, which runs as the application starts and again on every load of a hot-reload dylib, so the dylib's widgets select by these rules too. An application that starts its own runner calls it before running.
pub fn install() {
    set_plural_rules(&CldrPluralRules);
}

#[cfg(test)]
#[path = "plural_test.rs"]
mod tests;
