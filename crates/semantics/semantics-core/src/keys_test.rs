use super::*;

#[test]
fn a_set_survives_being_written_down_and_read_back() {
    for keys in [
        ConsumedKeys::EMPTY,
        ConsumedKeys::TAB,
        ConsumedKeys::SCROLLING,
        Role::MultilineTextInput.consumed_keys(),
        ConsumedKeys::ACTIVATION | ConsumedKeys::ARROW_DOWN,
    ] {
        assert_eq!(ConsumedKeys::from_names(&keys.to_names()), keys);
    }
}

#[test]
fn a_name_the_reader_does_not_know_leaves_the_rest_standing() {
    assert_eq!(
        ConsumedKeys::from_names("space  someday-key enter"),
        ConsumedKeys::ACTIVATION
    );
    assert_eq!(ConsumedKeys::from_names(""), ConsumedKeys::EMPTY);
}

#[test]
fn an_empty_set_is_written_as_nothing() {
    assert_eq!(ConsumedKeys::EMPTY.to_names(), "");
}

#[test]
fn only_the_multiline_editor_keeps_tab() {
    let keeping_tab: Vec<Role> = ALL_ROLES
        .into_iter()
        .filter(|role| role.consumed_keys().contains(ConsumedKeys::TAB))
        .collect();
    assert_eq!(keeping_tab, vec![Role::MultilineTextInput]);
}

#[test]
fn a_pressable_control_keeps_the_keys_that_press_it_and_leaves_scrolling_alone() {
    for role in [Role::Button, Role::Link, Role::CheckBox, Role::Switch] {
        let keys = role.consumed_keys();
        assert_eq!(keys, ConsumedKeys::ACTIVATION, "{role:?}");
        assert!(!keys.intersects(ConsumedKeys::ARROWS | ConsumedKeys::PAGING));
    }
}

#[test]
fn a_slider_keeps_the_arrows_and_not_space() {
    let keys = Role::Slider.consumed_keys();
    assert!(keys.contains(ConsumedKeys::ARROWS));
    assert!(!keys.intersects(ConsumedKeys::SPACE | ConsumedKeys::TAB));
}

#[test]
fn a_text_field_keeps_its_caret_keys() {
    let keys = Role::TextInput.consumed_keys();
    assert!(keys.contains(ConsumedKeys::ARROWS | ConsumedKeys::EDGES | ConsumedKeys::SPACE));
    assert!(keys.contains(ConsumedKeys::BACKSPACE));
}

#[test]
fn a_scroller_keeps_what_scrolls() {
    assert_eq!(Role::ScrollArea.consumed_keys(), ConsumedKeys::SCROLLING);
}

#[test]
fn what_is_only_read_keeps_nothing() {
    for role in ALL_ROLES
        .into_iter()
        .filter(|role| !role.is_control() && *role != Role::ScrollArea)
    {
        assert!(role.consumed_keys().is_empty(), "{role:?}");
    }
}

#[test]
fn every_control_keeps_something() {
    for role in ALL_ROLES.into_iter().filter(Role::is_control) {
        assert!(!role.consumed_keys().is_empty(), "{role:?}");
    }
}

const ALL_ROLES: [Role; 32] = [
    Role::Group,
    Role::Banner,
    Role::Navigation,
    Role::Main,
    Role::Complementary,
    Role::ContentInfo,
    Role::Article,
    Role::Section,
    Role::Form,
    Role::Search,
    Role::Heading(1),
    Role::List,
    Role::ListItem,
    Role::ScrollArea,
    Role::Drawing,
    Role::Dialog,
    Role::Button,
    Role::Link,
    Role::CheckBox,
    Role::Radio,
    Role::Switch,
    Role::Tab,
    Role::TabPanel,
    Role::MenuItem,
    Role::Slider,
    Role::SpinButton,
    Role::TextInput,
    Role::MultilineTextInput,
    Role::ComboBox,
    Role::Disclosure,
    Role::ProgressBar,
    Role::Label,
];

#[test]
fn a_group_name_reads_as_every_key_in_it() {
    assert_eq!(
        ConsumedKeys::from_names("arrows enter"),
        ConsumedKeys::ARROWS | ConsumedKeys::ENTER
    );
    assert_eq!(
        ConsumedKeys::named("scrolling"),
        Some(ConsumedKeys::SCROLLING)
    );
    assert_eq!(ConsumedKeys::named("none"), Some(ConsumedKeys::EMPTY));
    assert_eq!(ConsumedKeys::named("sideways"), None);
}
