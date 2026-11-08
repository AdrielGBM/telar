use super::PluralCategory::*;
use super::*;

fn builtin(locale: &str, count: i64) -> PluralCategory {
    BuiltinPluralRules.category(locale, count.into())
}

fn operands(text: &str) -> PluralOperands {
    PluralOperands::parse(text).unwrap()
}

#[test]
fn english_splits_one_from_the_rest() {
    assert_eq!(builtin("en", 0), Other);
    assert_eq!(builtin("en", 1), One);
    assert_eq!(builtin("en", 2), Other);
    // An unknown language falls back to this same rule.
    assert_eq!(builtin("zz", 1), One);
}

#[test]
fn a_region_subtag_resolves_like_its_language() {
    assert_eq!(builtin("pt-BR", 2), Other);
    assert_eq!(builtin("ru_RU", 2), Few);
}

#[test]
fn languages_without_a_plural_always_pick_other() {
    for n in [0, 1, 2, 11, 100] {
        assert_eq!(builtin("ja", n), Other, "{n}");
    }
}

#[test]
fn french_groups_zero_with_one() {
    assert_eq!(builtin("fr", 0), One);
    assert_eq!(builtin("fr", 1), One);
    assert_eq!(builtin("fr", 2), Other);
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
        assert_eq!(builtin("ru", n), want, "{n}");
    }
}

#[test]
fn polish_keeps_one_for_exactly_one() {
    assert_eq!(builtin("pl", 1), One);
    assert_eq!(builtin("pl", 21), Many);
    assert_eq!(builtin("pl", 22), Few);
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
        assert_eq!(builtin("ar", n), want, "{n}");
    }
}

#[test]
fn a_negative_count_is_read_by_magnitude() {
    assert_eq!(builtin("en", -1), One);
    assert_eq!(builtin("ru", -2), Few);
}

#[test]
fn categories_round_trip_through_their_names() {
    for c in [Zero, One, Two, Few, Many, Other] {
        assert_eq!(PluralCategory::parse(c.as_str()), Some(c));
    }
    assert_eq!(PluralCategory::parse("nav"), None);
}

#[test]
fn operands_keep_the_fraction_digits_as_written() {
    assert_eq!(
        operands("1.50"),
        PluralOperands {
            integer: 1,
            fraction: 50,
            fraction_digits: 2
        }
    );
    assert_eq!(operands("-3"), PluralOperands::from(3));
    assert!(operands("1").is_integer());
    assert!(!operands("1.0").is_integer());
}

#[test]
fn text_that_is_not_a_decimal_has_no_operands() {
    for text in [
        "",
        "one",
        "1.",
        ".5",
        "1e3",
        " 1",
        "1.2.3",
        "99999999999999999999",
    ] {
        assert_eq!(PluralOperands::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_builtin_rules_pick_other_for_a_fraction() {
    assert_eq!(BuiltinPluralRules.category("en", operands("1.0")), Other);
    assert_eq!(BuiltinPluralRules.category("ru", operands("1.5")), Other);
}

#[test]
fn with_no_rules_installed_the_builtin_rules_answer() {
    for (locale, n) in [("en", 1), ("ru", 21), ("pl", 22), ("ar", 11), ("ja", 1)] {
        assert_eq!(
            plural_category(locale, n),
            builtin(locale, n),
            "{locale} {n}"
        );
    }
}
