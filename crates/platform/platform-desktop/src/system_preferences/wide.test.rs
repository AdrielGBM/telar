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
