use super::*;

#[test]
fn switching_to_an_rtl_locale_flips_the_direction_and_back() {
    follow_locale_direction();
    i18n_core::set_locale(crate::locale_tag!("ar"));
    assert_eq!(ui_core::current_direction(), Direction::Rtl);
    i18n_core::set_locale(crate::locale_tag!("es"));
    assert_eq!(ui_core::current_direction(), Direction::Ltr);
}

#[test]
fn re_calling_replaces_the_previous_follower() {
    follow_locale_direction();
    let one_follower = reactive_core::live_effect_count();
    follow_locale_direction();
    assert_eq!(
        reactive_core::live_effect_count(),
        one_follower,
        "a leftover follower would set the direction a second time on every locale change"
    );
    i18n_core::set_locale(crate::locale_tag!("he"));
    assert_eq!(ui_core::current_direction(), Direction::Rtl);
    i18n_core::set_locale(crate::locale_tag!("en"));
    assert_eq!(ui_core::current_direction(), Direction::Ltr);
}
