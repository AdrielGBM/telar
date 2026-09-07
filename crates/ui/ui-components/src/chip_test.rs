use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{Event, PointerButton, PointerSource};
use renderer_core::DrawCommand;
use ui_core::{ComponentList, compute_layout, new_container, track_layout};

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
        (r.x + r.width - pad_x() - 3.0) as f64,
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
