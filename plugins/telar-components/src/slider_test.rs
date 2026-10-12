use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{moved, press, release};
use telar::{Component, LayoutItem, NodeId, Rect, signal};

use super::*;

fn lay_out(node: NodeId) -> Rect {
    telar::testing::lay_out(node, 300.0, 100.0)
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

#[test]
fn only_the_focused_slider_answers_the_arrows() {
    crate::test_support::fresh_layout_runtime();
    ui_core::reset_keyboard();
    telar::focus::clear();
    let (volume, brightness) = (signal(0.5f32), signal(0.5f32));
    let mut sliders: Vec<_> = [volume, brightness]
        .into_iter()
        .map(|value| {
            slider(
                SliderProps::props().value(value).step(0.1).max(1.0).build(),
                Children::default(),
            )
            .unwrap()
        })
        .collect();
    let right = telar::testing::named(telar::NamedKey::ArrowRight);

    for widget in &mut sliders {
        widget.on_event(&right);
    }
    assert_eq!(
        (volume.peek(), brightness.peek()),
        (0.5, 0.5),
        "nothing is focused"
    );

    telar::focus::focus_next();
    for widget in &mut sliders {
        widget.on_event(&right);
    }
    assert!((volume.peek() - 0.6).abs() < 1e-4, "{}", volume.peek());
    assert_eq!(brightness.peek(), 0.5, "the unfocused slider stayed put");
}

#[derive(Clone, Default)]
struct Plain {
    sliders: Option<SliderTokens>,
}

impl telar::ThemeTokens for Plain {
    fn border_width_strong(&self) -> f32 {
        2.5
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(sliders) = &self.sliders {
            extensions.insert(sliders.clone());
        }
    }
}

/// The track and the thumb a laid-out slider paints.
fn track_and_thumb(item: Box<dyn LayoutItem>) -> ((Rect, RectStyle), (Rect, RectStyle)) {
    telar::testing::lay_out(item.layout_node(), 400.0, 100.0);
    let painted: Vec<(Rect, RectStyle)> = telar::ComponentList::new(item)
        .commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } => Some((*rect, **style)),
            _ => None,
        })
        .collect();
    let thumb = *painted
        .iter()
        .find(|(_, style)| style.border.is_some())
        .expect("the thumb is ringed");
    (painted[0], thumb)
}

fn plain_slider(style: Option<telar::SurfaceAmendment>) -> Box<dyn LayoutItem> {
    let mut props = SliderProps::props().value(signal(0.5f32)).build();
    props.style = style;
    slider(props, Children::default()).unwrap()
}

#[test]
fn the_track_and_thumb_follow_the_roles_until_the_slider_tokens_name_their_own() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let ((track, track_style), (thumb, thumb_style)) = track_and_thumb(plain_slider(None));
    assert_eq!(track.height, crate::metrics::spacing());
    assert_eq!(
        track_style.radius,
        telar::BorderRadius::all(track.height / 2.0)
    );
    assert_eq!(thumb.width, crate::metrics::icon_size());
    assert_eq!(thumb_style.border.map(|b| b.widths[0]), Some(2.5));

    telar::set_theme(Plain {
        sliders: Some(SliderTokens {
            width: Some(300.0),
            track: Some(4.0),
            thumb: Some(20.0),
            ring: Some(1.0),
        }),
    });
    let ((track, _), (thumb, thumb_style)) = track_and_thumb(plain_slider(None));
    assert_eq!((track.width, track.height), (300.0, 4.0));
    assert_eq!(thumb.width, 20.0);
    assert_eq!(thumb_style.border.map(|b| b.widths[0]), Some(1.0));
}

#[test]
fn the_slider_scales_with_the_control_size_in_force() {
    crate::test_support::fresh_layout_runtime();
    let ((regular, _), (regular_thumb, _)) = track_and_thumb(plain_slider(None));
    crate::test_support::fresh_layout_runtime();
    let large =
        telar::provide_control_size(telar::ControlSize::Large, || Ok(plain_slider(None))).unwrap();
    let ((large, _), (large_thumb, _)) = track_and_thumb(Box::new(large));
    let scale = telar::ControlSize::Large.scale();
    assert_eq!(
        (large.width, large.height, large_thumb.width),
        (
            regular.width * scale,
            regular.height * scale,
            regular_thumb.width * scale
        )
    );
}

#[test]
fn a_style_amendment_reshapes_the_track() {
    crate::test_support::fresh_layout_runtime();
    let square: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(telar::BorderRadius::all(0.0)));
    let ((_, track), (_, thumb)) = track_and_thumb(plain_slider(Some(square)));
    assert_eq!(track.radius, telar::BorderRadius::all(0.0));
    assert_ne!(thumb.radius, telar::BorderRadius::all(0.0));
}

#[test]
fn a_press_beside_the_thin_track_still_takes_hold_of_it() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = slider(
        SliderProps::props().value(value).width(200.0).build(),
        Children::default(),
    )
    .unwrap();
    let rect = lay_out(widget.layout_node());
    let above = rect.y - (crate::metrics::MIN_TARGET - rect.height) / 2.0 + 1.0;
    widget.on_event(&press((rect.x + 100.0) as f64, above as f64));
    assert!((value.get() - 0.5).abs() < 1e-4, "got {}", value.get());
}
