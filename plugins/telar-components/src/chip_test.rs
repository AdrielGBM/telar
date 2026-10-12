use std::cell::Cell;
use std::rc::Rc;

use telar::{
    AvailableSpace, ComponentList, DrawCommand, Event, PointerButton, PointerSource,
    compute_layout, new_container, track_layout,
};

use super::*;

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

#[test]
fn renders_label() {
    crate::test_support::fresh_layout_runtime();
    let chip = chip(
        ChipProps::props().label("Draft").build(),
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[chip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
    let tree = ComponentList::new(chip);
    assert!(
        find_text(&tree.commands(), "Draft"),
        "a chip draws its label"
    );
}

#[test]
fn on_close_renders_and_fires_on_tap() {
    crate::test_support::fresh_layout_runtime();
    let flag = Rc::new(Cell::new(false));
    let sink = flag.clone();
    let mut chip = chip(
        ChipProps::props()
            .label("Tag")
            .on_close(Rc::new(move || sink.set(true)) as Rc<dyn Fn()>)
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = chip.layout_node();
    // A non-stretching root, so the pill shrink-wraps to its content: against a stretched pill the edge-tap math lands in dead space to the right of the ×.
    let rect = track_layout(node).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();

    let r = rect.get();
    let (cx, cy) = (
        (r.x + r.width - ChipTokens::current().pad_x() - 3.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    chip.on_event(&Event::PointerPressed {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    chip.on_event(&Event::PointerReleased {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert!(flag.get(), "tapping the × fires on_close");
}

#[test]
fn empty_label_builds_without_panic() {
    crate::test_support::fresh_layout_runtime();
    let chip = chip(ChipProps::props().build(), Children::default()).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[chip.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
    let tree = ComponentList::new(chip);
    let _ = tree.commands();
}

fn mounted(props: ChipProps) -> (ComponentList, telar::Rect) {
    crate::test_support::fresh_layout_runtime();
    let chip = chip(props, Children::default()).unwrap();
    let rect = track_layout(chip.layout_node()).unwrap();
    compute_layout(
        chip.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    (ComponentList::new(chip), rect.get())
}

#[test]
fn a_chip_is_a_pill_until_the_chip_tokens_name_a_radius() {
    let laid_out = Rc::new(Cell::new(telar::Rect::default()));
    let at = laid_out.clone();
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        move || {
            let (tree, rect) = mounted(ChipProps::props().label("Draft").build());
            at.set(rect);
            tree
        },
        |tree| crate::test_support::first_painted(tree).radius,
        move |_, _| shared::pill(laid_out.get()),
        |radius| ChipTokens {
            radius: Some(radius),
            ..ChipTokens::default()
        },
    );
}

#[test]
fn a_style_amendment_reshapes_the_pill() {
    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let (tree, _) = mounted(ChipProps::props().label("Draft").style(flat).build());
    let painted = crate::test_support::first_painted(&tree);
    assert_eq!(painted.radius, BorderRadius::all(0.0));
    assert!(painted.border.is_some(), "the rest of the paint is kept");
}

/// A chip's padding is a length at the regular size, so a larger control size pads it in proportion.
#[test]
fn the_padding_scales_with_the_control_size() {
    crate::test_support::fresh_layout_runtime();
    let chip = telar::provide_control_size(telar::ControlSize::Large, || {
        chip(
            ChipProps::props().label("Draft").build(),
            Children::default(),
        )
    })
    .unwrap();
    let rect = track_layout(chip.layout_node()).unwrap();
    compute_layout(
        chip.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let tree = ComponentList::new(chip);
    let label = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, rect, .. } if text.as_ref() == "Draft" => Some(*rect),
            _ => None,
        })
        .expect("the chip draws its label");
    assert_eq!(
        rect.get().width - label.width,
        2.0 * shared::gap_md() * 1.25
    );
}
