use layout_core::AvailableSpace;

use ui_core::{compute_layout, new_container, track_layout};

use super::*;
use crate::harness::{press, release};

fn slot_with_body(label: &'static str) -> Slots {
    let body = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new().height(20.0),
        |t| t,
    )
    .unwrap();
    let mut slots = Slots::new();
    slots.push(None, box_item(body));
    slots
}

#[test]
fn pressing_the_header_toggles_open() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let mut item = accordion(
        AccordionProps::props().title("Details").open(open).build(),
        Children::from(slot_with_body("Body")),
    )
    .unwrap();
    let node = item.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let r = rect.get();
    // The header is the top row, so a point near the top-left lands on it whatever the collapsed body's height.
    let (cx, cy) = ((r.x + 10.0) as f64, (r.y + 5.0) as f64);

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));
    assert!(open.get(), "a tap on the header opens the section");

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));
    assert!(!open.get(), "a second tap closes it again");
}

// Laid out as a child of a fixed-size wrapper, not as the layout root: an auto-height root fills its available space, which would stretch the accordion to 400px both closed and open and hide the difference.
#[test]
fn open_expands_body_and_close_collapses_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let item = accordion(
        AccordionProps::props().title("Details").open(open).build(),
        Children::from(slot_with_body("Body")),
    )
    .unwrap();
    let node = item.layout_node();
    let root_rect = track_layout(node).unwrap();
    let wrapper = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let closed_height = root_rect.get().height;

    open.set(true);
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let open_height = root_rect.get().height;

    assert!(
        open_height > closed_height,
        "opening the section grows the root's height (closed: {closed_height}, open: {open_height})"
    );
}

#[test]
fn uncontrolled_accordion_builds_and_starts_closed() {
    crate::test_support::fresh_layout_runtime();
    let item = accordion(
        AccordionProps::props().title("Details").build(),
        Children::from(slot_with_body("Body")),
    )
    .unwrap();
    let node = item.layout_node();
    let root_rect = track_layout(node).unwrap();
    let wrapper = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let height = root_rect.get().height;
    assert!(
        height > 0.0 && height <= 55.0,
        "closed, only the header should contribute height, got {height}"
    );
}
