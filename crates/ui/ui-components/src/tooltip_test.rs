use super::*;
use crate::harness::moved;
use layout_core::AvailableSpace;
use renderer_core::LineHeight;

use renderer_core::DrawCommand;
use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

fn slot_with_trigger() -> Slots {
    let inner = Container::new(LayoutStyle::new().width(80.0).height(30.0), vec![]).unwrap();
    let mut slots = Slots::new();
    slots.push(None, box_item(inner));
    slots
}

/// A hint is sized from the text it is describing, not from the theme: a compact panel that says its region is 11px gets a bubble at that panel's scale, and `TooltipProps` has no size to correct it with.
#[test]
fn a_bubble_takes_the_size_the_region_around_it_declared() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props().text("Move").build(),
        Children::from(slot_with_trigger()),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    ui_core::declare(
        root,
        renderer_core::Declared::default().with_font_size(11.0),
    );
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();

    let mut tree = ComponentList::new(tooltip);
    let _ = tree.commands();
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let size = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Move" => {
                Some(style.font_size)
            }
            _ => None,
        })
        .expect("the bubble drew its name");
    assert_eq!(size, 11.0 * BUBBLE_RATIO);
}

#[test]
fn hover_shows_bubble_and_leave_hides_it() {
    crate::test_support::fresh_layout_runtime();
    let slots = slot_with_trigger();
    let tooltip = tooltip(
        TooltipProps::props().text("Helpful hint").build(),
        Children::from(slots),
    )
    .unwrap();

    // A parent-less root computed against the window registers the overlay host the bubble anchors into.
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    let _ = tree.commands();

    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "leaving hides the bubble again"
    );

    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Helpful hint"),
        "bubble shows on hover"
    );

    tree.on_event(&moved(9999.0, 9999.0));
    relayout_if_dirty();
    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "bubble hidden on leave"
    );
}

/// The shape every hint in an application takes: the name, its binding pushed to the far edge, and a sentence under both. All three have to reach the bubble, because the reason the shortcut is a prop of its own is that folding it into the name loses exactly this arrangement.
#[test]
fn a_hint_shows_its_name_its_shortcut_and_its_description() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Move")
            .shortcut("G")
            .description("Drag the selection along the ground")
            .build(),
        Children::from(slot_with_trigger()),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let cmds = tree.commands();
    assert!(find_text(&cmds, "Move"), "the name");
    assert!(find_text(&cmds, "G"), "its binding");
    assert!(
        find_text(&cmds, "Drag the selection along the ground"),
        "and what it does"
    );
}

/// The description line is set with room to breathe, because it is the only part of a bubble that wraps.
///
/// At the default 1.2 the two lines of a long hint sit almost on top of each other, and it reads as the *text* being squashed rather than as the leading being short — which is why it only showed up on the hints long enough to need a second line, and looked like a placement bug rather than a type one. The looser leading is set on this line and on no other: the name and the shortcut never wrap.
#[test]
fn the_description_line_is_set_with_room_to_wrap_into() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Setup")
            .description("Name regions and say what it is made of")
            .build(),
        Children::from(slot_with_trigger()),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    // Asserted on the style, not the drawn height: whether this sentence needs a second line depends on the system font, so measuring it would test the CI runner's font.
    let leading_of = |needle: &str| {
        tree.commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text { text, style, .. } if text.starts_with(needle) => {
                    Some(style.line_height)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("`{needle}` is drawn"))
    };
    assert_eq!(
        leading_of("Name regions"),
        LineHeight::Times(DESCRIPTION_LEADING)
    );
    assert_eq!(
        leading_of("Setup"),
        LineHeight::Natural,
        "the name never wraps"
    );
}

/// A bubble is as wide as what it says, up to its cap. It was taking the cap whatever it had to say, because the layer its chip sits in was a `LayoutStyle::new()` — a CSS **block**, where a child fills its containing block and `align_items` means nothing. A two-word hint in a 240px box does not read as a hint, and nothing about it looks like a layout mode being wrong.
#[test]
fn a_bubble_is_as_wide_as_what_it_says() {
    crate::test_support::fresh_layout_runtime();
    let tooltip = tooltip(
        TooltipProps::props()
            .text("Object")
            .shortcut("1")
            .description("Pick whole bodies")
            .build(),
        Children::from(slot_with_trigger()),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[tooltip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(tooltip);
    tree.on_event(&moved(40.0, 15.0));
    relayout_if_dirty();

    let widest = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { rect, .. } => Some(rect.width),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(
        widest > 0.0 && widest < BUBBLE_MAX_WIDTH * 0.75,
        "the bubble hugged its text instead of taking its cap, got {widest}"
    );
}

/// A hint with nothing but a name is the common case, and it must not pay for the parts it left out with a taller bubble than it has words for.
#[test]
fn an_absent_shortcut_and_description_take_no_room() {
    crate::test_support::fresh_layout_runtime();
    let height_of = |props: TooltipProps| {
        let tooltip = tooltip(props, Children::from(slot_with_trigger())).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[tooltip.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(tooltip);
        tree.on_event(&moved(40.0, 15.0));
        relayout_if_dirty();
        tree.commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { rect, .. } => Some(rect.height),
                _ => None,
            })
            .sum::<f32>()
    };
    let bare = height_of(TooltipProps::props().text("Move").build());
    crate::test_support::fresh_layout_runtime();
    let full = height_of(
        TooltipProps::props()
            .text("Move")
            .description("Drag the selection")
            .build(),
    );
    assert!(bare > 0.0, "the name is drawn either way");
    assert!(
        full > bare,
        "a described hint is taller than a bare one: {bare} vs {full}"
    );
}

#[test]
fn builds_without_hover() {
    crate::test_support::fresh_layout_runtime();
    let slots = slot_with_trigger();
    let tooltip = tooltip(TooltipProps::props().build(), Children::from(slots)).unwrap();
    let tree = ComponentList::new(tooltip);
    assert!(
        !find_text(&tree.commands(), "Helpful hint"),
        "a tooltip draws nothing until it is hovered"
    );
}
