use super::*;

const CURRENT: &str =
    r#"<svg viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>"#;
const FIXED: &str = r##"<svg viewBox="0 0 24 24"><path fill="#1877f2" d="M0 0h24v24H0z"/></svg>"##;

fn info(palette: Option<bool>) -> SetInfo {
    SetInfo {
        palette,
        ..SetInfo::default()
    }
}

#[test]
fn a_set_declaring_no_palette_is_monochrome_whatever_its_markup() {
    assert!(is_monochrome(Some(&info(Some(false))), CURRENT));
    assert!(is_monochrome(Some(&info(Some(false))), FIXED));
}

#[test]
fn a_palette_set_keeps_its_colours_even_in_one_colour() {
    assert!(!is_monochrome(Some(&info(Some(true))), FIXED));
    assert!(!is_monochrome(Some(&info(Some(true))), CURRENT));
}

#[test]
fn a_set_that_says_nothing_is_judged_by_current_color() {
    for set in [None, Some(info(None))] {
        assert!(is_monochrome(set.as_ref(), CURRENT));
        assert!(!is_monochrome(set.as_ref(), FIXED));
    }
}

#[test]
fn current_color_is_found_in_any_case() {
    assert!(uses_current_color(r#"<path stroke="currentcolor"/>"#));
    assert!(uses_current_color(r#"<path style="fill:CurrentColor"/>"#));
    assert!(!uses_current_color(r##"<path fill="#000"/>"##));
}

#[test]
fn a_sourced_icon_answers_from_its_set_and_markup() {
    let icon = |set: Option<SetInfo>, svg: &str| SourcedIcon {
        svg: svg.to_string(),
        set,
        own: false,
        origin: String::new(),
    };
    assert!(icon(None, CURRENT).monochrome());
    assert!(!icon(Some(info(Some(true))), CURRENT).monochrome());
    assert!(icon(Some(info(Some(false))), FIXED).monochrome());
}
