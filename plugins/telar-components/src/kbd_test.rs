use super::*;
use telar::{AvailableSpace, ComponentList, Container, DrawCommand, compute_layout};

fn texts(chord: &'static str) -> Vec<String> {
    crate::test_support::fresh_layout_runtime();
    let cap = kbd(KbdProps::props().chord(chord).build(), Children::default()).unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(300.0).height(100.0),
        vec![cap],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let tree = ComponentList::new(box_item(root));
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_chord_splits_on_plus_into_one_cap_per_key() {
    assert_eq!(split_chord("Ctrl+K"), ["Ctrl", "K"]);
    assert_eq!(split_chord("Ctrl+Shift+P"), ["Ctrl", "Shift", "P"]);
    assert_eq!(split_chord("Esc"), ["Esc"]);
}

#[test]
fn a_plus_key_is_written_last() {
    assert_eq!(split_chord("Ctrl++"), ["Ctrl", "+"]);
    assert_eq!(split_chord("+"), ["+"]);
}

#[test]
fn spacing_and_empty_chords_leave_no_empty_caps() {
    assert_eq!(split_chord(" Shift + P "), ["Shift", "P"]);
    assert!(split_chord("").is_empty());
}

#[test]
fn mod_is_the_command_key_on_macos_and_control_elsewhere() {
    assert_eq!(display_name("Mod", true), "⌘");
    assert_eq!(display_name("Mod", false), "Ctrl");
    assert_eq!(display_name("mod", false), "Ctrl");
}

#[test]
fn modifier_names_become_symbols_on_macos_only() {
    assert_eq!(display_name("Shift", true), "⇧");
    assert_eq!(display_name("Alt", true), "⌥");
    assert_eq!(display_name("Shift", false), "Shift");
    assert_eq!(display_name("K", true), "K");
}

#[test]
fn each_key_is_drawn_on_a_cap_of_its_own() {
    assert_eq!(texts("Ctrl+K"), ["Ctrl", "K"]);
}
