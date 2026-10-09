use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{centre, press, release};
use telar::{Component, LayoutItem, NodeId, signal};

use super::*;

fn lay_out(node: NodeId) -> (f64, f64) {
    centre(telar::testing::lay_out(node, 200.0, 100.0))
}

#[test]
fn tap_sets_group_selection_to_value() {
    crate::test_support::fresh_layout_runtime();
    let selected = signal(0u32);
    let mut widget = radio(
        RadioProps::props()
            .selected(selected)
            .value(2)
            .label("Large")
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    assert_eq!(selected.get(), 0, "starts unselected (group is on 0)");
    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(selected.get(), 2, "a tap selects this button's value");
}

#[test]
fn on_select_reports_value() {
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = radio(
        RadioProps::props()
            .selected(signal(0u32))
            .value(5)
            .on_select(Rc::new(move |v| sink.set(Some(v))))
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(
        seen.get(),
        Some(5),
        "on_select fires with this button's value"
    );
}
