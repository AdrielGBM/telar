use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{press, release};
use telar::{AvailableSpace, Component, compute_layout, track_layout};

use super::*;

fn tap_minus(widget: &mut Box<dyn LayoutItem>, r: telar::Rect) {
    let (x, y) = (
        (r.x + shared::control_height() / 2.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    widget.on_event(&press(x, y));
    widget.on_event(&release(x, y));
}
fn tap_plus(widget: &mut Box<dyn LayoutItem>, r: telar::Rect) {
    let (x, y) = (
        (r.x + r.width - shared::control_height() / 2.0) as f64,
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
    let result = stepper(StepperProps::props().build());
    assert!(
        result.is_ok(),
        "an uncontrolled stepper builds from its default"
    );
}

#[test]
fn unset_max_does_not_clamp_to_min() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = stepper(StepperProps::props().value(value).step(1.0).build()).unwrap();
    let node = widget.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    let r = rect.get();

    for _ in 0..3 {
        tap_plus(&mut widget, r);
    }
    assert!((value.get() - 3.0).abs() < 1e-4, "got {}", value.get());
}

fn mounted(props: StepperProps) -> telar::ComponentList {
    crate::test_support::fresh_layout_runtime();
    let widget = stepper(props).unwrap();
    compute_layout(
        widget.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    telar::ComponentList::new(widget)
}

fn buttons(tree: &telar::ComponentList) -> Vec<RectStyle> {
    crate::test_support::drawn(Role::Button)
        .into_iter()
        .map(|at| crate::test_support::painted_at(tree, at))
        .collect()
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_stepper_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(StepperProps::props().build()),
        |tree| buttons(tree)[0].radius,
        |_, role| BorderRadius::all(role),
        |radius| StepperTokens {
            radius: Some(radius),
        },
    );
}

/// `@class` and the `style` prop amend both buttons, at rest and under the pointer.
#[test]
fn a_style_amendment_reshapes_both_buttons_in_every_state() {
    use telar::testing::{centre, moved, route};

    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let mut tree = mounted(StepperProps::props().style(flat).build());
    let resting = buttons(&tree);
    assert_eq!(resting.len(), 2);
    assert!(resting.iter().all(|b| b.radius == BorderRadius::all(0.0)));
    let (x, y) = centre(crate::test_support::drawn(Role::Button)[1]);
    route(&mut tree, &moved(x, y));
    let hovered = buttons(&tree)[1];
    assert_eq!(
        hovered.fill.map(|paint| paint.solid_color()),
        Some(shared::hovered_fill(shared::accent())),
        "the hovered button darkens like any filled control"
    );
    assert_eq!(
        hovered.radius,
        BorderRadius::all(0.0),
        "on the amended shape"
    );
}
