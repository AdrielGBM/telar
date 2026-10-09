use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{centre, press, release};
use telar::{Component, LayoutItem, NodeId, signal};

use super::*;

fn lay_out(node: NodeId) -> (f64, f64) {
    centre(telar::testing::lay_out(node, 200.0, 100.0))
}

#[test]
fn tap_toggles_bound_signal() {
    crate::test_support::fresh_layout_runtime();
    let checked = signal(false);
    let mut widget = checkbox(
        CheckboxProps::props()
            .checked(checked)
            .label("Agree")
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert!(checked.get(), "a tap turns the checkbox on");

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert!(!checked.get(), "a second tap turns it back off");
}

#[test]
fn on_toggle_reports_new_state() {
    let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = checkbox(
        CheckboxProps::props()
            .on_toggle(Rc::new(move |v| sink.set(Some(v))))
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(
        seen.get(),
        Some(true),
        "on_toggle fires with the new checked state"
    );
}
