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

#[test]
fn a_chord_is_spoken_by_the_names_on_its_caps() {
    assert_eq!(spoken_chord("Shift+Alt+P"), "Shift+Alt+P");
    assert_eq!(spoken_chord(" Ctrl + K "), "Ctrl+K");
    let command = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    };
    assert_eq!(spoken_chord("Mod+K"), format!("{command}+K"));
}

#[test]
fn a_reader_hears_the_chord_once_and_not_each_cap() {
    crate::test_support::fresh_layout_runtime();
    let caps = kbd(
        KbdProps::props().chord("Shift+P").build(),
        Children::default(),
    )
    .unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(300.0).height(100.0),
        vec![caps],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let tree = ComponentList::new(box_item(root));
    let read: Vec<String> = ui_core::accessibility::snapshot(&tree.commands())
        .into_iter()
        .map(|node| node.name)
        .collect();
    assert_eq!(read, ["Shift+P"]);
}

fn mounted(props: KbdProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let caps = kbd(props, Children::default()).unwrap();
    compute_layout(
        caps.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    ComponentList::new(caps)
}

/// Every cap's paint, in the order the keys are pressed.
fn caps(tree: &ComponentList) -> Vec<RectStyle> {
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } if style.fill.is_some() => Some(**style),
            _ => None,
        })
        .collect()
}

#[test]
fn the_corner_is_the_theme_s_item_role_until_the_kbd_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(KbdProps::props().chord("Ctrl+K").build()),
        |tree| caps(tree)[0].radius,
        |_, role| BorderRadius::all(role),
        |radius| KbdTokens {
            radius: Some(radius),
            ..KbdTokens::default()
        },
    );
}

#[test]
fn a_style_amendment_reshapes_every_cap() {
    let flat: SurfaceAmendment =
        std::rc::Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let tree = mounted(KbdProps::props().chord("Ctrl+K").style(flat).build());
    let painted = caps(&tree);
    assert_eq!(painted.len(), 2);
    assert!(
        painted
            .iter()
            .all(|cap| cap.radius == BorderRadius::all(0.0))
    );
}

#[test]
fn a_one_letter_cap_is_never_narrower_than_it_is_tall() {
    crate::test_support::fresh_layout_runtime();
    let caps = kbd(KbdProps::props().chord("K").build(), Children::default()).unwrap();
    compute_layout(
        caps.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let tree = ComponentList::new(caps);
    let cap = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { rect, style, .. } if style.fill.is_some() => Some(*rect),
            _ => None,
        })
        .expect("the key is drawn on a cap");
    assert!(cap.width >= cap.height, "{cap:?}");
}
