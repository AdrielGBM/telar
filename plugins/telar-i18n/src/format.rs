use fixed_decimal::Decimal;
use icu_calendar::{Date, Gregorian, Iso};
use icu_datetime::FixedCalendarDateTimeFormatter;
use icu_datetime::fieldsets::YMD;
use icu_datetime::options::Length;
use icu_decimal::DecimalFormatter;

use crate::locale::active_locale;

/// Formats `number` for the active locale: its digits, decimal separator and grouping (`1,234.5` in English, `1.234,5` in German, `١٬٢٣٤٫٥` in Egyptian Arabic).
///
/// Reads the locale reactively, so a widget calling this in its content closure re-renders on a language switch. Integers convert directly; a float goes through [`Decimal::try_from_f64`] with the precision the caller means, and a decimal written as text through [`Decimal::try_from_str`], which keeps trailing zeros.
pub fn format_number(number: impl Into<Decimal>) -> String {
    let number = number.into();
    match DecimalFormatter::try_new((&active_locale()).into(), Default::default()) {
        Ok(formatter) => formatter.format(&number).to_string(),
        Err(_) => number.to_string(),
    }
}

/// Formats `date` as year, month and day for the active locale, at `length` (`3/15/24`, `Mar 15, 2024`, `March 15, 2024` in English).
///
/// Always in the Gregorian calendar, whichever calendar the locale prefers: that keeps every other calendar's names and conversion code out of the binary. Reads the locale reactively, like [`format_number`].
pub fn format_date(date: Date<Iso>, length: Length) -> String {
    let date = date.to_calendar(Gregorian);
    match FixedCalendarDateTimeFormatter::<Gregorian, YMD>::try_new(
        (&active_locale()).into(),
        YMD::for_length(length),
    ) {
        Ok(formatter) => formatter.format(&date).to_string(),
        Err(_) => format!(
            "{:04}-{:02}-{:02}",
            date.year().extended_year(),
            date.month().ordinal,
            date.day_of_month().0
        ),
    }
}

#[cfg(test)]
#[path = "format_test.rs"]
mod tests;
