use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;

use ui_core::{Component, compute_layout, track_layout};

use super::*;
use crate::harness::{press, release};

fn tap_minus(widget: &mut Box<dyn LayoutItem>, r: geometry_core::Rect) {
    let (x, y) = (
        (r.x + button_size() / 2.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    widget.on_event(&press(x, y));
    widget.on_event(&release(x, y));
}
fn tap_plus(widget: &mut Box<dyn LayoutItem>, r: geometry_core::Rect) {
    let (x, y) = (
        (r.x + r.width - button_size() / 2.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    widget.on_event(&press(x, y));
    widget.on_event(&release(x, y));
}

#[test]
fn plus_increments_by_step_and_clamps_at_max() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(4.0f32);
    let mut widget = stepper(
        StepperProps::props()
            .value(value)
            .min(0.0)
            .max(5.0)
            .step(1.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let r = rect.get();

    tap_plus(&mut widget, r);
    assert!((value.get() - 5.0).abs() < 1e-4, "got {}", value.get());

    tap_plus(&mut widget, r);
    assert!(
        (value.get() - 5.0).abs() < 1e-4,
        "+ must clamp at max, got {}",
        value.get()
    );
}

#[test]
fn minus_decrements_by_step_and_clamps_at_min() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(1.0f32);
    let mut widget = stepper(
        StepperProps::props()
            .value(value)
            .min(0.0)
            .max(5.0)
            .step(1.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let r = rect.get();

    tap_minus(&mut widget, r);
    assert!((value.get() - 0.0).abs() < 1e-4, "got {}", value.get());

    tap_minus(&mut widget, r);
    assert!(
        (value.get() - 0.0).abs() < 1e-4,
        "- must clamp at min, got {}",
        value.get()
    );
}

#[test]
fn on_change_fires_with_new_value() {
    let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = stepper(
        StepperProps::props()
            .min(0.0)
            .max(5.0)
            .step(1.0)
            .on_change(Rc::new(move |v| sink.set(v)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let r = rect.get();

    tap_plus(&mut widget, r);
    assert!(
        (seen.get() - 1.0).abs() < 1e-4,
        "on_change should see the new clamped value, got {}",
        seen.get()
    );
}

#[test]
fn uncontrolled_stepper_builds_with_default_value() {
    crate::test_support::fresh_layout_runtime();
    let result = stepper(StepperProps::props().build(), Children::default());
    assert!(
        result.is_ok(),
        "an uncontrolled stepper builds from its default"
    );
}

#[test]
fn unset_max_does_not_clamp_to_min() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = stepper(
        StepperProps::props().value(value).step(1.0).build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let r = rect.get();

    for _ in 0..3 {
        tap_plus(&mut widget, r);
    }
    assert!((value.get() - 3.0).abs() < 1e-4, "got {}", value.get());
}
