use super::*;
use crate::FloatPrecision;
use telar::set_locale;

fn ides_of_march() -> Date<Iso> {
    Date::try_new_iso(2024, 3, 15).unwrap()
}

#[test]
fn numbers_take_the_active_locale_separators_and_digits() {
    let cases = [
        ("en", "1,234,567", "-1,234.50"),
        ("de", "1.234.567", "-1.234,50"),
        ("hi", "12,34,567", "-1,234.50"),
        ("ar-EG", "١٬٢٣٤٬٥٦٧", "\u{61c}-١٬٢٣٤٫٥٠"),
    ];
    for (locale, integer, decimal) in cases {
        set_locale(locale);
        assert_eq!(format_number(1_234_567), integer, "{locale}");
        assert_eq!(
            format_number(Decimal::try_from_str("-1234.50").unwrap()),
            decimal,
            "{locale}"
        );
    }
}

#[test]
fn a_float_formats_at_the_precision_it_was_converted_with() {
    set_locale("de");
    let tenth = Decimal::try_from_f64(0.1, FloatPrecision::RoundTrip).unwrap();
    assert_eq!(format_number(tenth), "0,1");
}

#[test]
fn dates_take_the_active_locale_pattern_at_each_length() {
    let cases = [
        ("en", ["3/15/24", "Mar 15, 2024", "March 15, 2024"]),
        ("de", ["15.03.24", "15.03.2024", "15. März 2024"]),
        ("es", ["15/3/24", "15 mar 2024", "15 de marzo de 2024"]),
    ];
    for (locale, expected) in cases {
        set_locale(locale);
        for (length, want) in [Length::Short, Length::Medium, Length::Long]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                format_date(ides_of_march(), length),
                want,
                "{locale} {length:?}"
            );
        }
    }
}

#[test]
fn a_tag_written_with_an_underscore_resolves_like_one_with_a_dash() {
    set_locale("ru_RU");
    assert_eq!(
        format_date(ides_of_march(), Length::Long),
        "15 марта 2024\u{202f}г."
    );
}

#[test]
fn a_tag_icu4x_cannot_place_formats_with_the_root_patterns() {
    set_locale("not a locale");
    assert_eq!(format_number(1_234_567), "1,234,567");
    assert_eq!(format_date(ides_of_march(), Length::Short), "2024-03-15");
}
