use super::*;

#[test]
fn a_role_reads_back_as_the_name_it_was_written_with() {
    for name in [
        "banner",
        "navigation",
        "main",
        "complementary",
        "contentinfo",
        "article",
        "section",
        "form",
        "search",
        "list",
        "listitem",
        "button",
        "checkbox",
        "radio",
        "switch",
        "tab",
        "tabpanel",
        "menuitem",
        "slider",
        "spinbutton",
        "combobox",
        "progressbar",
        "label",
        "group",
    ] {
        let role = Role::parse(name).unwrap_or_else(|| panic!("`{name}` should parse"));
        assert_eq!(role.as_str(), name, "`{name}` should round-trip");
    }
}

#[test]
fn the_words_an_author_reaches_for_first_point_at_the_role_they_mean() {
    assert_eq!(Role::parse("nav"), Some(Role::Navigation));
    assert_eq!(Role::parse("sidebar"), Some(Role::Complementary));
    assert_eq!(Role::parse("header"), Some(Role::Banner));
    assert_eq!(Role::parse("footer"), Some(Role::ContentInfo));
    assert_eq!(Role::parse("h2"), Some(Role::Heading(2)));
}

#[test]
fn a_name_nothing_answers_to_is_not_guessed_at() {
    assert_eq!(Role::parse("aricle"), None);
    assert_eq!(Role::parse(""), None);
}

#[test]
fn a_region_is_not_a_control_and_neither_is_a_plain_box() {
    assert!(
        Role::Navigation.is_region(),
        "a navigation landmark is a region"
    );
    assert!(
        !Role::Navigation.is_control(),
        "and a region is not something you operate"
    );
    assert!(Role::Slider.is_control(), "a slider is a control");
    assert!(!Role::Slider.is_region(), "and a control is not a landmark");
    assert!(
        !Role::Group.is_region(),
        "a plain group is neither a region"
    );
    assert!(!Role::Group.is_control(), "nor a control");
}

#[test]
fn a_box_is_a_group_until_it_says_otherwise() {
    assert_eq!(Semantics::default().role, Role::Group);
    assert_eq!(Semantics::of(Role::Main).role, Role::Main);
    assert!(
        Semantics::group().label.is_none(),
        "a group says nothing about itself until labelled"
    );
}
