use platform_core::ColorScheme;

use super::*;

#[test]
fn a_dark_background_index_is_a_dark_scheme() {
    assert_eq!(colorfgbg_scheme("15;0"), Some(ColorScheme::Dark));
    assert_eq!(colorfgbg_scheme("7;8"), Some(ColorScheme::Dark));
    assert_eq!(colorfgbg_scheme("15;default;0"), Some(ColorScheme::Dark));
}

#[test]
fn a_light_background_index_is_a_light_scheme() {
    assert_eq!(colorfgbg_scheme("0;15"), Some(ColorScheme::Light));
    assert_eq!(colorfgbg_scheme("0;7"), Some(ColorScheme::Light));
}

#[test]
fn an_unreadable_hint_is_unknown() {
    assert_eq!(colorfgbg_scheme(""), None);
    assert_eq!(colorfgbg_scheme("15;default"), None);
    assert_eq!(colorfgbg_scheme("15;300"), None);
}
