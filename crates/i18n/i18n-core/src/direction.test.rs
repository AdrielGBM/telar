use super::*;

#[test]
fn right_to_left_languages_are_recognised_with_and_without_a_region() {
    for tag in ["ar", "ar-EG", "he_IL", "fa", "ur-PK", "HE"] {
        assert_eq!(Direction::for_locale(tag), Direction::Rtl, "{tag}");
    }
}

#[test]
fn languages_written_in_several_scripts_default_to_left_to_right() {
    for tag in ["ha", "ha-NG", "uz", "uz_UZ", "az", "pa"] {
        assert_eq!(Direction::for_locale(tag), Direction::Ltr, "{tag}");
    }
}

#[test]
fn a_right_to_left_script_subtag_decides() {
    for tag in [
        "uz-Arab",
        "az-Arab",
        "az_Arab_IR",
        "pa-arab",
        "ff-Adlm",
        "man-Nkoo",
    ] {
        assert_eq!(Direction::for_locale(tag), Direction::Rtl, "{tag}");
    }
}

#[test]
fn a_left_to_right_script_subtag_decides() {
    for tag in ["sr-Cyrl", "uz-Latn", "az-Cyrl-AZ", "ar-Latn", "zh-Hant-TW"] {
        assert_eq!(Direction::for_locale(tag), Direction::Ltr, "{tag}");
    }
}

#[test]
fn everything_else_is_left_to_right() {
    for tag in ["en", "es-AR", "ja", "", "zz"] {
        assert_eq!(Direction::for_locale(tag), Direction::Ltr, "{tag}");
    }
}
