use super::*;

fn multi_sz(entries: &[&str]) -> Vec<u16> {
    let mut buffer = Vec::new();
    for entry in entries {
        buffer.extend(entry.encode_utf16());
        buffer.push(0);
    }
    buffer.push(0);
    buffer
}

#[test]
fn every_entry_is_read_in_order() {
    assert_eq!(
        split_multi_sz(&multi_sz(&["es-CL", "en-US", "fr"])),
        ["es-CL", "en-US", "fr"]
    );
}

#[test]
fn the_empty_list_is_empty() {
    assert!(split_multi_sz(&multi_sz(&[])).is_empty());
    assert!(split_multi_sz(&[]).is_empty());
}

#[test]
fn nothing_past_the_terminator_is_read() {
    let mut buffer = multi_sz(&["de-DE"]);
    buffer.extend("junk".encode_utf16());
    assert_eq!(split_multi_sz(&buffer), ["de-DE"]);
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn intl_names_the_locale_settings_in_any_case() {
    assert!(names_locale_settings(&wide("intl")));
    assert!(names_locale_settings(&wide("Intl")));
}

#[test]
fn other_setting_areas_are_not_the_locale() {
    assert!(!names_locale_settings(&wide("ImmersiveColorSet")));
    assert!(!names_locale_settings(&wide("intlx")));
    assert!(!names_locale_settings(&[]));
}

#[test]
fn the_high_contrast_and_animation_actions_are_relevant_whatever_the_area() {
    assert!(is_relevant_setting_change(0x0043, || wide("")));
    assert!(is_relevant_setting_change(0x1043, || wide("Policy")));
}

#[test]
fn the_locale_area_is_relevant_whatever_the_action() {
    assert!(is_relevant_setting_change(0, || wide("intl")));
    assert!(is_relevant_setting_change(0x2000, || wide("INTL")));
}

#[test]
fn another_action_in_another_area_is_not_relevant() {
    assert!(!is_relevant_setting_change(0, || wide("ImmersiveColorSet")));
    assert!(!is_relevant_setting_change(0, Vec::new));
}

#[test]
fn the_area_is_not_read_when_the_action_decides() {
    assert!(is_relevant_setting_change(0x0043, || unreachable!(
        "the area is only read when the action is not enough"
    )));
}
