use super::*;
use telar::{
    AvailableSpace, ComponentList, Container, DrawCommand, KeyboardConvention, compute_layout,
    set_keyboard_convention,
};

fn texts(chord: &'static str) -> Vec<String> {
    texts_in(KeyboardConvention::Pc, chord)
}

fn texts_in(convention: KeyboardConvention, chord: &'static str) -> Vec<String> {
    crate::test_support::fresh_layout_runtime();
    set_keyboard_convention(convention);
    let tree = telar::testing::scoped(|| {
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
        ComponentList::new(box_item(root))
    });
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn each_key_is_drawn_on_a_cap_of_its_own_as_a_pc_names_it() {
    assert_eq!(texts("Mod+Shift+P"), ["Ctrl", "Shift", "P"]);
    assert_eq!(texts("Ctrl++"), ["Ctrl", "+"]);
    assert_eq!(texts("Esc"), ["Esc"]);
}

#[test]
fn a_mac_draws_its_symbols_on_the_caps() {
    assert_eq!(
        texts_in(KeyboardConvention::Mac, "Mod+Shift+P"),
        ["⇧", "⌘", "P"]
    );
    assert_eq!(texts_in(KeyboardConvention::Mac, "Alt+↑"), ["⌥", "↑"]);
}

#[test]
fn text_that_names_no_chord_is_one_cap_as_written() {
    assert_eq!(texts(" Hyper+K "), ["Hyper+K"]);
    assert!(texts("  ").is_empty());
    assert_eq!(shown_chord(" Hyper+K "), "Hyper+K");
}

#[test]
fn a_chord_is_spoken_as_the_platform_writes_it() {
    crate::test_support::fresh_layout_runtime();
    assert_eq!(shown_chord("Shift+Alt+P"), "Alt+Shift+P");
    assert_eq!(shown_chord(" Ctrl + K "), "Ctrl+K");
    assert_eq!(shown_chord("Mod+K"), "Ctrl+K");
    set_keyboard_convention(KeyboardConvention::Mac);
    assert_eq!(shown_chord("Mod+K"), "⌘K");
    assert_eq!(shown_chord("Mod+Shift+P"), "⇧⌘P");
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
    let read: Vec<String> = telar::inspect::snapshot(&tree.commands())
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

fn first_cap(tree: &ComponentList) -> telar::Rect {
    tree.commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { rect, style } if style.fill.is_some() => Some(*rect),
            _ => None,
        })
        .expect("a kbd paints its caps")
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
        |tree, role| {
            let cap = first_cap(tree);
            BorderRadius::all(role).fitted(cap.width, cap.height)
        },
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
