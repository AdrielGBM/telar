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
        "tablist",
        "tab",
        "tabpanel",
        "menuitem",
        "slider",
        "spinbutton",
        "combobox",
        "progressbar",
        "tree",
        "treeitem",
        "toolbar",
        "separator",
        "status",
        "log",
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
fn the_role_says_what_its_on_off_state_is() {
    for role in [Role::CheckBox, Role::Radio, Role::Switch] {
        assert_eq!(role.toggle_kind(), Some(ToggleKind::Checked), "{role:?}");
    }
    assert_eq!(
        Role::Button.toggle_kind(),
        Some(ToggleKind::Pressed),
        "a button with a state is a toggle button, pressed rather than checked"
    );
    assert_eq!(Role::Tab.toggle_kind(), Some(ToggleKind::Selected));
    assert_eq!(
        Role::TreeItem.toggle_kind(),
        Some(ToggleKind::Selected),
        "a tree row's on/off state is being the chosen one; whether it is open is a state of its own"
    );
    assert_eq!(Role::Disclosure.toggle_kind(), Some(ToggleKind::Expanded));
    for role in [Role::Link, Role::Slider, Role::Navigation, Role::Group] {
        assert_eq!(role.toggle_kind(), None, "{role:?} has no on/off state");
    }
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

#[test]
fn an_annotation_names_a_box_over_what_the_widget_derived() {
    let derived = Semantics::drawing();
    assert!(derived.label.is_none(), "a picture has no name of its own");
    let annotation = Annotation {
        label: Some("Company logo".into()),
        lang: Some("es".into()),
        hidden: false,
        anchor: Some("logo".into()),
    };
    let said = derived.annotated(&annotation);
    assert_eq!(said.role, Role::Drawing, "the role is still the widget's");
    assert_eq!(said.label.as_deref(), Some("Company logo"));
    assert_eq!(said.lang.as_deref(), Some("es"));
    assert_eq!(said.anchor.as_deref(), Some("logo"));
    assert!(!said.hidden);
}

#[test]
fn an_empty_annotation_changes_nothing_and_hiding_is_only_ever_added() {
    let derived = Semantics::of(Role::Button)
        .with_label("Close")
        .hidden_from_readers();
    assert!(Annotation::default().is_empty());
    assert_eq!(derived.clone().annotated(&Annotation::default()), derived);
    assert!(
        derived.annotated(&Annotation::default()).hidden,
        "an annotation that does not hide cannot unhide what the widget hid"
    );
}

#[test]
fn a_language_is_an_annotation_even_without_a_name() {
    let annotation = Annotation {
        lang: Some("ja".into()),
        ..Annotation::default()
    };
    assert!(!annotation.is_empty());
    let said = Semantics::group().annotated(&annotation);
    assert_eq!(said.lang.as_deref(), Some("ja"));
    assert!(said.label.is_none());
}

#[test]
fn only_a_link_with_somewhere_to_go_is_current() {
    let link = Semantics::group().linking_to(Destination::anchor("web"));
    assert_eq!(link.current_kind(), None, "not marked");
    assert_eq!(
        link.marked_current(true).current_kind(),
        Some(CurrentKind::Location)
    );
    assert_eq!(
        Semantics::of(Role::Link)
            .marked_current(true)
            .current_kind(),
        None,
        "a disabled link goes nowhere, so it is the current one of nothing"
    );
    assert_eq!(
        Semantics::of(Role::Button)
            .marked_current(true)
            .current_kind(),
        None
    );
}

#[test]
fn a_tree_row_carries_its_openness_and_its_place_beside_its_selection() {
    let row = Semantics {
        toggled: Some(true),
        expanded: Some(false),
        position: Some(SetPosition {
            level: 2,
            position: 3,
            size: 7,
        }),
        ..Semantics::of(Role::TreeItem)
    };
    assert_ne!(
        row,
        Semantics {
            expanded: Some(true),
            ..row.clone()
        },
        "opening a row is a change a target has to act on"
    );
    assert_eq!(
        Semantics::default().expanded,
        None,
        "a box says nothing about being open until it can be"
    );
    assert_eq!(Semantics::default().position, None);
}

#[test]
fn a_reading_is_compared_by_its_numbers() {
    let reading = |now: f64| NumericValue {
        now,
        min: 0.0,
        max: 10.0,
    };
    let with = |now: f64| Semantics {
        value: Some(reading(now)),
        orientation: Some(Orientation::Vertical),
        ..Semantics::of(Role::Splitter)
    };
    assert_eq!(with(4.0), with(4.0));
    assert_ne!(with(4.0), with(5.0));
    assert_ne!(with(4.0), Semantics::of(Role::Splitter));
    assert_eq!(Orientation::Vertical.as_str(), "vertical");
    assert_eq!(Orientation::Horizontal.as_str(), "horizontal");
}

#[test]
fn the_box_a_cursor_rests_on_is_part_of_what_a_container_says() {
    let on = |row: Option<u64>| Semantics {
        active_descendant: row,
        ..Semantics::of(Role::Tree)
    };
    assert_eq!(Semantics::default().active_descendant, None);
    assert_ne!(
        on(Some(4)),
        on(Some(5)),
        "a cursor moving to another row is a change a target has to act on"
    );
    assert_ne!(on(Some(4)), on(None));
    assert_eq!(
        on(Some(4))
            .annotated(&Annotation {
                label: Some("Files".into()),
                ..Annotation::default()
            })
            .active_descendant,
        Some(4),
        "naming the container keeps where its cursor is"
    );
}

#[test]
fn only_a_role_less_box_around_exactly_one_control_lends_its_name() {
    assert!(Role::Group.lends_name_to_control(false, 1));
    assert!(!Role::Group.lends_name_to_control(true, 1));
    assert!(!Role::Group.lends_name_to_control(false, 0));
    assert!(!Role::Group.lends_name_to_control(false, 2));
    assert!(!Role::TabList.lends_name_to_control(false, 1));
}
