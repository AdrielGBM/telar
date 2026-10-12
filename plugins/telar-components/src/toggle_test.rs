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

#[derive(Clone, Default)]
struct Plain {
    toggles: Option<ToggleTokens>,
}

impl telar::ThemeTokens for Plain {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(toggles) = &self.toggles {
            extensions.insert(toggles.clone());
        }
    }
}

/// A primary as light as a theme might pick for its accent, in either scheme, with a dark ink on it.
#[derive(Clone)]
struct Butter;

impl telar::ThemeTokens for Butter {
    fn primary(&self) -> Color {
        Color::rgba(1.0, 0.88, 0.45, 1.0)
    }
    fn on_primary(&self) -> Color {
        Color::rgba(0.12, 0.1, 0.05, 1.0)
    }
}

/// What a laid-out item paints as boxes, in paint order.
fn rects(item: Box<dyn LayoutItem>) -> Vec<(telar::Rect, RectStyle)> {
    telar::testing::lay_out(item.layout_node(), 200.0, 100.0);
    telar::ComponentList::new(item)
        .commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } => Some((*rect, **style)),
            _ => None,
        })
        .collect()
}

/// The track and the knob, the two filled boxes a switch paints, in that order.
fn track_and_knob(
    item: Box<dyn LayoutItem>,
) -> ((telar::Rect, RectStyle), (telar::Rect, RectStyle)) {
    let filled: Vec<_> = rects(item)
        .into_iter()
        .filter(|(_, style)| style.fill.is_some())
        .collect();
    assert_eq!(filled.len(), 2, "a switch paints its track and its knob");
    (filled[0], filled[1])
}

fn switch(on: bool) -> Box<dyn LayoutItem> {
    toggle(
        ToggleProps::props().checked(signal(on)).build(),
        Children::default(),
    )
    .unwrap()
}

fn solid(style: &RectStyle) -> Color {
    style.fill.expect("filled").solid_color()
}

#[test]
fn the_knob_stands_out_from_its_track_in_either_scheme_and_on_a_light_primary() {
    for dark in [false, true] {
        for light_primary in [false, true] {
            crate::test_support::fresh_layout_runtime();
            telar::register_mode("light", || {});
            telar::register_mode("dark", || {});
            telar::follow_system("light", "dark");
            telar::set_scheme_preference(if dark {
                telar::SchemePreference::Dark
            } else {
                telar::SchemePreference::Light
            });
            assert_eq!(telar::is_dark(), dark);
            if light_primary {
                telar::set_theme(Butter);
            } else {
                telar::set_theme(Plain::default());
            }
            for on in [false, true] {
                let ((_, track), (_, knob)) = track_and_knob(switch(on));
                let under = crate::shared::over(solid(&track), crate::shared::surface());
                let ratio = under.contrast_ratio(solid(&knob));
                assert!(
                    ratio >= 3.0,
                    "dark {dark}, light primary {light_primary}, on {on}: the knob reads at {ratio:.2}:1 on its track"
                );
            }
        }
    }
}

#[test]
fn the_knob_is_the_theme_s_thumb_wherever_that_reads() {
    crate::test_support::fresh_layout_runtime();
    telar::set_scheme_preference(telar::SchemePreference::Light);
    telar::set_theme(Plain::default());
    let (_, (_, knob)) = track_and_knob(switch(true));
    assert_eq!(solid(&knob), telar::use_theme_tokens().thumb());
}

#[test]
fn the_track_is_sized_from_the_knob_and_follows_the_tokens() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let ((track, style), (knob, _)) = track_and_knob(switch(false));
    let (icon, inset) = (crate::metrics::icon_size(), crate::metrics::gap_xs());
    assert_eq!((knob.width, knob.height), (icon, icon));
    assert_eq!(
        (track.width, track.height),
        (icon * 2.0 + inset * 2.0, icon + inset * 2.0)
    );
    assert_eq!(
        style.radius,
        BorderRadius::all(track.height / 2.0),
        "a pill"
    );

    telar::set_theme(Plain {
        toggles: Some(ToggleTokens {
            knob: Some(20.0),
            inset: Some(3.0),
            radius: Some(5.0),
        }),
    });
    let ((track, style), (knob, knob_style)) = track_and_knob(switch(false));
    assert_eq!((knob.width, knob.height), (20.0, 20.0));
    assert_eq!((track.width, track.height), (46.0, 26.0));
    assert_eq!(style.radius, BorderRadius::all(5.0));
    assert_eq!(
        knob_style.radius,
        BorderRadius::all(2.0),
        "the knob's corner is concentric with the track's"
    );
}

#[test]
fn the_switch_scales_with_the_control_size_in_force() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let ((regular, _), _) = track_and_knob(switch(false));
    crate::test_support::fresh_layout_runtime();
    let large =
        telar::provide_control_size(telar::ControlSize::Large, || Ok(switch(false))).unwrap();
    let ((large, _), _) = track_and_knob(Box::new(large));
    let scale = telar::ControlSize::Large.scale();
    assert_eq!(
        (large.width, large.height),
        (regular.width * scale, regular.height * scale)
    );
}

#[test]
fn a_style_amendment_reshapes_the_track_on_and_off() {
    let square: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    for on in [false, true] {
        crate::test_support::fresh_layout_runtime();
        telar::set_theme(Plain::default());
        let item = toggle(
            ToggleProps::props()
                .checked(signal(on))
                .style(square.clone())
                .build(),
            Children::default(),
        )
        .unwrap();
        let ((_, track), (_, knob)) = track_and_knob(item);
        assert_eq!(track.radius, BorderRadius::all(0.0));
        assert_ne!(
            knob.radius,
            BorderRadius::all(0.0),
            "the knob is not the principal surface"
        );
    }
}
