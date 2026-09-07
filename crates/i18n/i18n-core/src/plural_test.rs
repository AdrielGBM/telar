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
