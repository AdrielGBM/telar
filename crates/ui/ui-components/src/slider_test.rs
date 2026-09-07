use std::cell::Cell;
use std::rc::Rc;

use geometry_core::Rect;

use reactive_core::signal;
use ui_core::{Component, LayoutItem, NodeId};

use super::*;
use crate::harness::{moved, press, release};

fn lay_out(node: NodeId) -> Rect {
    crate::harness::lay_out(node, 300.0, 100.0)
}

#[test]
fn drag_to_midpoint_sets_value_half() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = slider(
        SliderProps::props().value(value).width(200.0).build(),
        Children::default(),
    )
    .unwrap();
    let rect = lay_out(widget.layout_node());

    widget.on_event(&press((rect.x + 100.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (value.get() - 0.5).abs() < 1e-4,
        "midpoint press should map to 0.5, got {}",
        value.get()
    );

    widget.on_event(&moved((rect.x + 200.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (value.get() - 1.0).abs() < 1e-4,
        "dragging to the far edge should clamp to 1.0, got {}",
        value.get()
    );
    widget.on_event(&release((rect.x + 200.0) as f64, (rect.y + 4.0) as f64));
}

// An unset `value` prop must fall back to a working internal signal, not panic.
#[test]
fn uncontrolled_slider_builds_with_default_value() {
    crate::test_support::fresh_layout_runtime();
    let result = slider(SliderProps::props().build(), Children::default());
    assert!(
        result.is_ok(),
        "an uncontrolled slider builds from its default"
    );
}

#[test]
fn on_change_fires_with_mapped_value() {
    let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = slider(
        SliderProps::props()
            .width(100.0)
            .on_change(Rc::new(move |v| sink.set(v)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let rect = lay_out(widget.layout_node());

    widget.on_event(&press((rect.x + 25.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (seen.get() - 0.25).abs() < 1e-4,
        "on_change should see the same mapped value, got {}",
        seen.get()
    );
}

#[test]
fn custom_range_maps_midpoint_to_range_midpoint() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = slider(
        SliderProps::props()
            .value(value)
            .width(200.0)
            .min(0.0)
            .max(100.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    let rect = lay_out(widget.layout_node());

    widget.on_event(&press((rect.x + 100.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (value.get() - 50.0).abs() < 1e-3,
        "midpoint press over 0..100 should map to 50.0, got {}",
        value.get()
    );
}

#[test]
fn step_snaps_value_to_nearest_step() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = slider(
        SliderProps::props()
            .value(value)
            .width(100.0)
            .min(0.0)
            .max(100.0)
            .step(10.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    let rect = lay_out(widget.layout_node());

    // px=23 -> raw 23.0 -> 2.3 steps, which rounds down to the 20.0 step.
    widget.on_event(&press((rect.x + 23.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (value.get() - 20.0).abs() < 1e-4,
        "a drag near 0.23 with step 10 should snap to 20.0, got {}",
        value.get()
    );
}

#[test]
fn label_builds_without_panicking() {
    crate::test_support::fresh_layout_runtime();
    let result = slider(
        SliderProps::props().label("Volume").build(),
        Children::default(),
    );
    assert!(result.is_ok(), "a labelled slider builds");
}
