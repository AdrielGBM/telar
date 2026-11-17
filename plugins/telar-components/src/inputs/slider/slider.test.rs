use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{moved, press, release};
use telar::{Component, LayoutItem, NodeId, Rect, signal};

use super::*;

fn lay_out(node: NodeId) -> Rect {
    telar::testing::lay_out(node, 300.0, 100.0)
}

/// A slider its call places `width` wide.
fn placed(props: SliderProps, width: f32) -> Box<dyn LayoutItem> {
    let item = slider(props).unwrap();
    telar::place_item(item.layout_node(), move |style| style.width(width));
    item
}

#[test]
fn drag_to_midpoint_sets_value_half() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = placed(SliderProps::props().value(value).build(), 200.0);
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
    let result = slider(SliderProps::props().build());
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
    let mut widget = placed(
        SliderProps::props()
            .on_change(Rc::new(move |v| sink.set(v)))
            .build(),
        100.0,
    );
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
    let mut widget = placed(
        SliderProps::props().value(value).range(0.0..=100.0).build(),
        200.0,
    );
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
    let mut widget = placed(
        SliderProps::props()
            .value(value)
            .range(0.0..=100.0)
            .step(10.0)
            .build(),
        100.0,
    );
    let rect = lay_out(widget.layout_node());

    // px=23 -> raw 23.0 -> 2.3 steps, which rounds down to the 20.0 step.
    widget.on_event(&press((rect.x + 23.0) as f64, (rect.y + 4.0) as f64));
    assert!(
        (value.get() - 20.0).abs() < 1e-4,
        "a drag near 0.23 with step 10 should snap to 20.0, got {}",
        value.get()
    );
}

/// A column narrower than the track's length narrows the track instead of letting it overflow, captioned or not, and a press is measured against the length it was laid out to.
#[test]
fn a_column_narrower_than_the_track_narrows_it() {
    for label in ["", "Volume"] {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps::props().value(value).label(label).build()).unwrap();
        assert!(SliderTokens::current().width(ControlSize::Regular) > 120.0);
        let rect = telar::testing::lay_out(widget.layout_node(), 120.0, 100.0);
        assert_eq!(rect.width, 120.0, "{label:?}");

        let track_y = rect.y + rect.height - 2.0;
        widget.on_event(&press((rect.x + 60.0) as f64, track_y as f64));
        assert!(
            (value.get() - 0.5).abs() < 1e-4,
            "{label:?}: got {}",
            value.get()
        );
    }
}

#[test]
fn label_builds_without_panicking() {
    crate::test_support::fresh_layout_runtime();
    let result = slider(SliderProps::props().label("Volume").build());
    assert!(result.is_ok(), "a labelled slider builds");
}

#[test]
fn only_the_focused_slider_answers_the_arrows() {
    crate::test_support::fresh_layout_runtime();
    telar::testing::reset_keyboard();
    telar::focus::clear();
    let (volume, brightness) = (signal(0.5f32), signal(0.5f32));
    let mut sliders: Vec<_> = [volume, brightness]
        .into_iter()
        .map(|value| slider(SliderProps::props().value(value).step(0.1).build()).unwrap())
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

/// A reader steps a slider as its arrow keys do and writes a number into it as a drag would land it, each reported to the caller.
#[test]
fn a_reader_steps_the_slider_and_sets_it_to_a_step() {
    use telar::focus::AccessAction;
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let value = signal(0.5f32);
    let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
    let sink = seen.clone();
    let _widget = slider(
        SliderProps::props()
            .value(value)
            .step(0.1)
            .on_change(Rc::new(move |v| sink.set(v)))
            .build(),
    )
    .unwrap();
    let id = telar::focus::tab_order()[0].id;

    assert!(telar::focus::perform(id, AccessAction::Increment));
    assert!((value.peek() - 0.6).abs() < 1e-4, "{}", value.peek());
    assert!((seen.get() - 0.6).abs() < 1e-4);
    assert!(telar::focus::perform(id, AccessAction::Decrement));
    assert!((value.peek() - 0.5).abs() < 1e-4, "{}", value.peek());

    let to = |now: f64| {
        AccessAction::SetValue(telar::Value::from(
            telar::NumericValue::new(0.0..=1.0, now).expect("a reading within its range"),
        ))
    };
    assert!(telar::focus::perform(id, to(0.83)));
    assert!((value.peek() - 0.8).abs() < 1e-4, "{}", value.peek());
    assert!(
        !telar::focus::perform(
            id,
            AccessAction::SetValue(telar::Value::Text("loud".into()))
        ),
        "a slider holds no text"
    );
}

#[test]
fn home_end_and_the_page_keys_move_the_focused_slider() {
    crate::test_support::fresh_layout_runtime();
    telar::testing::reset_keyboard();
    telar::focus::clear();
    let value = signal(5.0f32);
    let mut widget = slider(SliderProps::props().value(value).range(0.0..=100.0).build()).unwrap();
    telar::focus::focus_next();
    let mut press_key = |key| widget.on_event(&telar::testing::named(key));

    press_key(telar::NamedKey::PageUp);
    assert!((value.peek() - 15.0).abs() < 1e-4, "{}", value.peek());
    press_key(telar::NamedKey::PageDown);
    press_key(telar::NamedKey::PageDown);
    assert_eq!(value.peek(), 0.0, "a page stops at the start");
    press_key(telar::NamedKey::End);
    assert_eq!(value.peek(), 100.0);
    press_key(telar::NamedKey::Home);
    assert_eq!(value.peek(), 0.0);
}

#[test]
fn a_page_of_a_stepped_slider_is_whole_steps() {
    crate::test_support::fresh_layout_runtime();
    telar::testing::reset_keyboard();
    telar::focus::clear();
    let value = signal(0.0f32);
    let mut widget = slider(
        SliderProps::props()
            .value(value)
            .range(0.0..=100.0)
            .step(3.0)
            .build(),
    )
    .unwrap();
    telar::focus::focus_next();

    widget.on_event(&telar::testing::named(telar::NamedKey::PageUp));
    assert_eq!(value.peek(), 9.0);
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
fn track_and_thumb(
    build: impl FnOnce() -> Box<dyn LayoutItem>,
) -> ((Rect, RectStyle), (Rect, RectStyle)) {
    let tree = telar::testing::scoped(|| {
        let item = build();
        telar::testing::lay_out(item.layout_node(), 400.0, 100.0);
        telar::ComponentList::new(item)
    });
    let painted: Vec<(Rect, RectStyle)> = tree
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
    slider(props).unwrap()
}

#[test]
fn the_track_and_thumb_follow_the_roles_until_the_slider_tokens_name_their_own() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let ((track, track_style), (thumb, thumb_style)) = track_and_thumb(|| plain_slider(None));
    assert_eq!(
        track.height,
        crate::metrics::spacing(telar::ControlSize::Regular)
    );
    assert_eq!(
        track_style.radius,
        telar::BorderRadius::all(track.height / 2.0)
    );
    assert_eq!(
        thumb.width,
        crate::metrics::icon_size(telar::ControlSize::Regular)
    );
    assert_eq!(thumb_style.border.map(|b| b.widths[0]), Some(2.5));

    telar::set_theme(Plain {
        sliders: Some(SliderTokens {
            width: Some(300.0),
            track: Some(4.0),
            thumb: Some(20.0),
            ring: Some(1.0),
        }),
    });
    let ((track, _), (thumb, thumb_style)) = track_and_thumb(|| plain_slider(None));
    assert_eq!((track.width, track.height), (300.0, 4.0));
    assert_eq!(thumb.width, 20.0);
    assert_eq!(thumb_style.border.map(|b| b.widths[0]), Some(1.0));
}

/// The track's thickness and the thumb scale with the slider's size, and its length follows the text, as a field's does.
#[test]
fn the_slider_scales_with_its_size() {
    crate::test_support::fresh_layout_runtime();
    let ((regular, _), (regular_thumb, _)) = track_and_thumb(|| plain_slider(None));
    for size in [ControlSize::Small, ControlSize::Large] {
        crate::test_support::fresh_layout_runtime();
        let ((track, _), (thumb, _)) = track_and_thumb(|| {
            slider(
                SliderProps::props()
                    .value(signal(0.5f32))
                    .size(size)
                    .build(),
            )
            .unwrap()
        });
        assert_eq!(
            (track.width, track.height, thumb.width),
            (
                shared::measure(size, WIDTH_EMS),
                regular.height * size.scale(),
                regular_thumb.width * size.scale()
            ),
            "{size:?}"
        );
    }
}

#[test]
fn a_style_amendment_reshapes_the_track() {
    crate::test_support::fresh_layout_runtime();
    let square: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(telar::BorderRadius::all(0.0)));
    let ((_, track), (_, thumb)) = track_and_thumb(|| plain_slider(Some(square)));
    assert_eq!(track.radius, telar::BorderRadius::all(0.0));
    assert_ne!(thumb.radius, telar::BorderRadius::all(0.0));
}

#[test]
fn a_press_beside_the_thin_track_still_takes_hold_of_it() {
    crate::test_support::fresh_layout_runtime();
    let value = signal(0.0f32);
    let mut widget = placed(SliderProps::props().value(value).build(), 200.0);
    let rect = lay_out(widget.layout_node());
    let above = rect.y - (crate::metrics::MIN_TARGET - rect.height) / 2.0 + 1.0;
    widget.on_event(&press((rect.x + 100.0) as f64, above as f64));
    assert!((value.get() - 0.5).abs() < 1e-4, "got {}", value.get());
}

#[test]
fn a_range_with_nothing_to_slide_over_or_a_step_that_moves_nothing_is_refused() {
    crate::test_support::fresh_layout_runtime();
    assert!(slider(SliderProps::props().range(1.0..=0.0).build()).is_err());
    assert!(slider(SliderProps::props().range(2.0..=2.0).build()).is_err());
    assert!(slider(SliderProps::props().step(-1.0).build()).is_err());
}

#[test]
fn a_transparent_track_and_fill_are_drawn_transparent_and_absent_colours_follow_the_theme() {
    crate::test_support::fresh_layout_runtime();
    let ((_, track), _) = track_and_thumb(|| {
        slider(
            SliderProps::props()
                .value(signal(0.5f32))
                .color(Color::TRANSPARENT)
                .track_color(Color::TRANSPARENT)
                .build(),
        )
        .unwrap()
    });
    assert_eq!(
        track.fill.map(|paint| paint.solid_color()),
        Some(Color::TRANSPARENT)
    );
    let ((_, track), (_, thumb)) = track_and_thumb(|| plain_slider(None));
    assert_eq!(
        track.fill.map(|paint| paint.solid_color()),
        Some(shared::muted())
    );
    assert_eq!(
        thumb.fill.map(|paint| paint.solid_color()),
        Some(shared::accent())
    );
}
