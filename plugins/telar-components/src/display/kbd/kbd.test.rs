use telar::{
    AvailableSpace, BorderRadius, ComponentList, Container, ControlSize, DrawCommand,
    KeyboardConvention, LayoutItem, LayoutStyle, RectStyle, SurfaceAmendment, box_item,
    compute_layout, set_keyboard_convention,
};

use crate::shared::shown_chord;
use crate::{KbdCaps, KbdProps, KbdTokens, kbd};

fn texts(chord: &'static str) -> Vec<String> {
    texts_in(KeyboardConvention::Pc, chord)
}

fn texts_in(convention: KeyboardConvention, chord: &'static str) -> Vec<String> {
    crate::test_support::fresh_layout_runtime();
    set_keyboard_convention(convention);
    let tree = telar::testing::scoped(|| {
        let cap = kbd(KbdProps::props().chord(chord).build()).unwrap();
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

fn mounted(props: KbdProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let caps = kbd(props).unwrap();
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
    let caps = kbd(KbdProps::props().chord("K").build()).unwrap();
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

/// The caps, their text and the gap between them all follow the chord's own size.
#[test]
fn the_caps_scale_with_the_kbd_s_size() {
    let laid = |size: ControlSize| {
        let tree = mounted(KbdProps::props().chord("Ctrl+K").size(size).build());
        let rects: Vec<telar::Rect> = tree
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Rect { rect, style } if style.fill.is_some() => Some(*rect),
                _ => None,
            })
            .collect();
        let gap = rects[1].x - (rects[0].x + rects[0].width);
        (rects[0].height, gap, crate::metrics::gap_xs(size))
    };
    let [small, regular, large] =
        [ControlSize::Small, ControlSize::Regular, ControlSize::Large].map(laid);
    assert!(
        small.0 < regular.0 && regular.0 < large.0,
        "{small:?} {regular:?} {large:?}"
    );
    for (_, gap, wanted) in [small, regular, large] {
        assert!((gap - wanted).abs() <= 0.5, "{gap} snapped from {wanted}");
    }
}

/// By default a cap takes the ink of the place the chord sits in, so in a dark tooltip its key reads in the tooltip's light ink, on a wash of that ink, rather than in the page's.
#[test]
fn a_cap_takes_the_ink_of_the_place_it_sits_in() {
    crate::test_support::fresh_layout_runtime();
    let light = telar::Color::rgb(0.95, 0.95, 0.97);
    let chord = kbd(KbdProps::props().chord("K").build()).unwrap();
    let place = Container::new(LayoutStyle::new(), vec![chord])
        .unwrap()
        .declaring(move || telar::Declared::default().with_color(light));
    compute_layout(
        place.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let tree = ComponentList::new(box_item(place));
    let ink = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { style, .. } => Some(style.color),
            _ => None,
        })
        .expect("the key is drawn");
    assert_eq!(ink, telar::Paint::Solid(light));
    let fill = caps(&tree)[0]
        .fill
        .expect("the cap is filled")
        .solid_color();
    assert_eq!((fill.r, fill.g, fill.b), (light.r, light.g, light.b));
}

#[test]
fn each_way_of_putting_keys_on_caps_follows_the_platform() {
    let drawn = |convention, chord: &'static str, caps: KbdCaps| {
        crate::test_support::fresh_layout_runtime();
        set_keyboard_convention(convention);
        let chord = kbd(KbdProps::props().chord(chord).caps(caps).build()).unwrap();
        compute_layout(
            chord.layout_node(),
            AvailableSpace::MaxContent,
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let tree = ComponentList::new(chord);
        tree.commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (pc, mac) = (KeyboardConvention::Pc, KeyboardConvention::Mac);
    let chord = "Mod+Shift+P Mod+A";
    assert_eq!(
        drawn(pc, chord, KbdCaps::Each),
        ["Ctrl", "Shift", "P", "Ctrl", "A"]
    );
    assert_eq!(
        drawn(pc, chord, KbdCaps::Linked),
        ["Ctrl", "+", "Shift", "+", "P", "Ctrl", "+", "A"]
    );
    assert_eq!(
        drawn(pc, chord, KbdCaps::Joined),
        ["Ctrl + Shift + P", "Ctrl + A"]
    );
    assert_eq!(
        drawn(mac, chord, KbdCaps::Linked),
        ["⇧", "⌘", "P", "⌘", "A"]
    );
    assert_eq!(drawn(mac, chord, KbdCaps::Joined), ["⇧⌘P", "⌘A"]);
}

#[test]
fn a_sequence_is_drawn_step_after_step_and_heard_once() {
    assert_eq!(texts("Mod+K S"), ["Ctrl", "K", "S"]);
    assert_eq!(texts(" Ctrl + K  Ctrl + + "), ["Ctrl", "K", "Ctrl", "+"]);
    assert_eq!(shown_chord("Mod+K S"), "Ctrl+K, then S");
}

#[test]
fn text_with_a_word_that_names_no_key_stays_one_cap() {
    assert_eq!(texts("Any key"), ["Any key"]);
}

#[test]
fn a_step_that_names_no_chord_is_kept_as_written_beside_the_others() {
    assert_eq!(texts("Mod+Shift+P a+a"), ["Ctrl", "Shift", "P", "a+a"]);
    assert_eq!(shown_chord("Mod+Shift+P a+a"), "Ctrl+Shift+P, then a+a");
}
