use std::cell::Cell;
use std::rc::Rc;

use reactive_core::signal;
use ui_core::{Component, LayoutItem, NodeId};

use super::*;
use crate::harness::{centre, press, release};

fn lay_out(node: NodeId) -> (f64, f64) {
    centre(crate::harness::lay_out(node, 200.0, 100.0))
}

#[test]
fn tap_toggles_bound_signal() {
    crate::test_support::fresh_layout_runtime();
    let on = signal(false);
    let mut widget = toggle(
        ToggleProps::props()
            .checked(on)
            .label("Notifications")
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert!(on.get(), "a tap switches it on");

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert!(!on.get(), "a second tap switches it back off");
}

#[test]
fn on_toggle_reports_new_state() {
    let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = toggle(
        ToggleProps::props()
            .on_toggle(Rc::new(move |v| sink.set(Some(v))))
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(seen.get(), Some(true), "on_toggle fires with the new state");
}
