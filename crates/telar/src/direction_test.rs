use super::*;

#[test]
fn switching_to_an_rtl_locale_flips_the_direction_and_back() {
    follow_locale_direction();
    i18n_core::set_locale("ar");
    assert_eq!(ui_core::current_direction(), Direction::Rtl);
    i18n_core::set_locale("es");
    assert_eq!(ui_core::current_direction(), Direction::Ltr);
}
