use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{press, release};
use telar::{AvailableSpace, Component, compute_layout, track_layout};

use super::*;

fn tap_minus(widget: &mut Box<dyn LayoutItem>, r: telar::Rect) {
    let (x, y) = (
        (r.x + shared::control_height(telar::ControlSize::Regular) / 2.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    widget.on_event(&press(x, y));
    widget.on_event(&release(x, y));
}
fn tap_plus(widget: &mut Box<dyn LayoutItem>, r: telar::Rect) {
    let (x, y) = (
        (r.x + r.width - shared::control_height(telar::ControlSize::Regular) / 2.0) as f64,
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
            .range(0.0..=5.0)
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
            .range(0.0..=5.0)
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
            .range(0.0..=5.0)
            .step(1.0)
            .on_change(move |v| sink.set(v))
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
fn a_stepper_whose_range_names_no_end_is_unbounded() {
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

/// Where the − and + boxes are drawn and how they are painted: the boxes the stepper fills, in the order they are drawn.
fn buttons_drawn(tree: &telar::ComponentList) -> Vec<(telar::Rect, RectStyle)> {
    let mut filled = Vec::new();
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, style, .. } = command
            && style.fill.is_some()
        {
            filled.push((telar::transform_clip_rect(matrix, *rect), **style));
        }
    });
    filled
}

fn buttons(tree: &telar::ComponentList) -> Vec<RectStyle> {
    buttons_drawn(tree)
        .into_iter()
        .map(|(_, style)| style)
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
    let (x, y) = centre(buttons_drawn(&tree)[1].0);
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

#[test]
fn a_range_whose_start_lies_past_its_end_or_a_step_that_moves_nothing_is_refused() {
    crate::test_support::fresh_layout_runtime();
    assert!(stepper(StepperProps::props().range(5.0..=1.0).build()).is_err());
    assert!(stepper(StepperProps::props().step(0.0).build()).is_err());
}

#[test]
fn the_buttons_take_the_colour_given_even_a_transparent_one_and_the_accent_when_none_is() {
    let fill = |tree: &telar::ComponentList| buttons(tree)[0].fill.map(|paint| paint.solid_color());
    let tree = mounted(StepperProps::props().color(Color::TRANSPARENT).build());
    assert_eq!(fill(&tree), Some(Color::TRANSPARENT));
    drop(tree);
    let tree = mounted(StepperProps::props().build());
    assert_eq!(fill(&tree), Some(shared::accent()));
}

fn spin_button(props: StepperProps) -> (telar::ComponentList, telar::focus::FocusId) {
    telar::focus::clear();
    let tree = mounted(props);
    let stops = telar::focus::tab_order();
    assert_eq!(
        stops.len(),
        1,
        "the stepper is one stop, its − and + are not"
    );
    assert_eq!(stops[0].role, Role::SpinButton);
    (tree, stops[0].id)
}

/// A reader steps the stepper as its − and + do, and the value never leaves the range.
#[test]
fn a_reader_steps_the_stepper_by_one_step_within_its_range() {
    use telar::focus::AccessAction;
    let value = signal(4.0f32);
    let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
    let sink = seen.clone();
    let (_tree, id) = spin_button(
        StepperProps::props()
            .value(value)
            .range(0.0..=5.0)
            .step(1.0)
            .on_change(move |v| sink.set(v))
            .build(),
    );

    assert!(telar::focus::perform(id, AccessAction::Increment));
    assert_eq!(value.peek(), 5.0);
    assert_eq!(seen.get(), 5.0, "the caller hears the step");
    assert!(
        !telar::focus::perform(id, AccessAction::Increment),
        "at the end of its range the value cannot move"
    );
    assert_eq!(value.peek(), 5.0);
    assert!(telar::focus::perform(id, AccessAction::Decrement));
    assert_eq!(value.peek(), 4.0);
}

/// A number a reader hands the stepper lands on its steps and inside its range; text is refused.
#[test]
fn a_reader_sets_the_stepper_to_the_nearest_step() {
    use telar::focus::AccessAction;
    let value = signal(1.0f32);
    let (_tree, id) = spin_button(
        StepperProps::props()
            .value(value)
            .range(1.0..=9.0)
            .step(2.0)
            .build(),
    );
    let to = |now: f64| {
        AccessAction::SetValue(telar::Value::from(
            telar::NumericValue::new(f64::NEG_INFINITY..=f64::INFINITY, now)
                .expect("a finite reading"),
        ))
    };
    assert!(telar::focus::perform(id, to(5.8)));
    assert_eq!(value.peek(), 5.0, "rounded to the steps counted from 1");
    assert!(telar::focus::perform(id, to(40.0)));
    assert_eq!(value.peek(), 9.0, "kept to the range");
    assert!(
        !telar::focus::perform(id, AccessAction::SetValue(telar::Value::Text("ten".into()))),
        "a stepper holds no text"
    );
}

/// The arrow keys step the focused stepper, and a reader is told its value and its name.
#[test]
fn the_arrow_keys_step_the_focused_stepper_and_a_reader_hears_its_value() {
    let value = signal(2.0f32);
    let (mut tree, id) = spin_button(StepperProps::props().value(value).range(0.0..=10.0).build());
    telar::testing::reset_keyboard();
    telar::focus::request(id);
    for key in [NamedKey::ArrowUp, NamedKey::ArrowUp, NamedKey::ArrowLeft] {
        telar::testing::route(&mut tree, &telar::testing::named(key));
    }
    assert_eq!(value.peek(), 3.0);
    let spin = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::SpinButton)
        .expect("the stepper is a spin button");
    assert_eq!(
        spin.value
            .and_then(|value| value.reading())
            .map(|r| r.now()),
        Some(3.0)
    );
}
