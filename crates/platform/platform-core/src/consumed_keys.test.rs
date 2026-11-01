use super::*;
use crate::Role;

fn named(key: NamedKey) -> Key {
    Key::Named(key)
}

#[test]
fn a_character_is_never_kept_from_the_host() {
    assert!(!consumes(ConsumedKeys::SCROLLING, &Key::Char('a')));
    assert!(!consumes(
        Role::MultilineTextInput.consumed_keys(),
        &Key::Char(' ')
    ));
}

#[test]
fn a_key_no_host_acts_on_is_never_kept() {
    assert!(key_member(&named(NamedKey::Escape)).is_empty());
    assert!(key_member(&named(NamedKey::F5)).is_empty());
}

#[test]
fn a_slider_keeps_the_arrows_and_lets_tab_and_space_through() {
    let keys = Role::Slider.consumed_keys();
    for arrow in [
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
    ] {
        assert!(consumes(keys, &named(arrow.clone())), "{arrow:?}");
    }
    assert!(!consumes(keys, &named(NamedKey::Tab)));
    assert!(!consumes(keys, &named(NamedKey::Space)));
    assert!(!consumes(keys, &named(NamedKey::PageDown)));
}

#[test]
fn a_button_keeps_what_presses_it() {
    let keys = Role::Button.consumed_keys();
    assert!(consumes(keys, &named(NamedKey::Space)));
    assert!(consumes(keys, &named(NamedKey::Enter)));
    assert!(consumes(keys, &named(NamedKey::NumpadEnter)));
    assert!(!consumes(keys, &named(NamedKey::ArrowDown)));
}

#[test]
fn a_text_field_keeps_its_caret_keys_but_not_tab() {
    let keys = Role::TextInput.consumed_keys();
    for key in [
        NamedKey::ArrowLeft,
        NamedKey::Home,
        NamedKey::End,
        NamedKey::Space,
        NamedKey::Backspace,
    ] {
        assert!(consumes(keys, &named(key.clone())), "{key:?}");
    }
    assert!(!consumes(keys, &named(NamedKey::Tab)));
}

#[test]
fn nothing_focused_keeps_nothing() {
    for key in [NamedKey::Tab, NamedKey::Space, NamedKey::PageDown] {
        assert!(!consumes(ConsumedKeys::EMPTY, &named(key)));
    }
}
