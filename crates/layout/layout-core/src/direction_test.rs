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
