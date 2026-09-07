use reactive_core::signal;
use ui_core::NodeId;

use super::*;

fn lay_out(node: NodeId) {
    crate::harness::lay_out(node, 300.0, 100.0);
}

#[test]
fn uncontrolled_progress_builds_with_default_value() {
    crate::test_support::fresh_layout_runtime();
    let result = progress(ProgressProps::props().build(), Children::default());
    assert!(
        result.is_ok(),
        "an uncontrolled progress builds from its default"
    );
}

#[test]
fn controlled_progress_builds_and_layouts() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.3f32);
    let widget = progress(
        ProgressProps::props()
            .value(value)
            .width(200.0)
            .height(10.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(widget.layout_node());
    assert_eq!(value.get(), 0.3);
}

#[test]
fn setting_value_after_build_does_not_panic() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let widget = progress(
        ProgressProps::props().value(value).build(),
        Children::default(),
    )
    .unwrap();
    lay_out(widget.layout_node());
    value.set(0.75);
    value.set(1.5);
    assert_eq!(value.get(), 1.5);
}

/// The reason this prop is a closure. A reading derived from two services has no signal behind it, and a bar that insisted on one is a bar an application reimplements next to the catalogue.
#[test]
fn a_derived_reading_can_drive_the_bar() {
    reactive_core::reset_runtime();
    let used = signal(3.0f32);
    let total = signal(4.0f32);
    let fraction = reactive_core::derive_pair(used, total, |u, t| u / t);
    let _read = fraction;
    let bar = progress(
        ProgressProps::props().value(fraction).stretch(true).build(),
        Children::default(),
    );
    assert!(bar.is_ok(), "a derivation drives it");
    used.set(1.0);
    assert_eq!(fraction.get(), 0.25, "and keeps following its sources");
}
