use std::rc::Rc;

use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use ui_core::focus::Role;
use ui_core::{LayoutItem, StyledContainer, box_item, box_transform};

use crate::shared;

/// Track thickness (px) — a slim pill rail with a floating thumb.
fn track_height() -> f32 {
    shared::spacing()
}
/// Thumb diameter (px), bigger than the track so it stays easy to grab; it overhangs the rail on purpose.
fn thumb_size() -> f32 {
    shared::icon_size()
}

fn thumb_box() -> LayoutStyle {
    LayoutStyle::new().width(thumb_size()).height(thumb_size())
}
fn track_box(width: f32) -> LayoutStyle {
    LayoutStyle::new().width(width).height(track_height())
}

/// A drag-driven `min..=max` control: a rounded track, an accent fill up to `value`, and a thumb positioned by
/// it. This is the canonical demo of the `on_drag` primitive (see `StyledContainer::on_drag`): the widget just
/// encapsulates the press/move -> value mapping (and the fill/thumb repaint) an app would otherwise wire up by
/// hand. High-level sugar over the primitives; lives in `ui-components`, not the kernel, so an app can drop it.
/// `value` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own
/// signal), `Some` is caller-bound.
pub struct SliderProps {
    /// Bound value, reported in `min..=max` (out-of-range inputs are clamped on every drag report, not here).
    /// `None` (the default) is uncontrolled — the widget makes its own `signal(min)`.
    pub value: Option<RwSignal<f32>>,
    /// Fill/thumb accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    pub track_color: Box<dyn Fn() -> Color>,
    /// Track width in px. `0.0` (the default) means "unset" — the slider uses `220.0`.
    pub width: f32,
    /// Lower bound of the reported value. Default `0.0`.
    pub min: f32,
    /// Upper bound of the reported value. Default `1.0`. If `max <= min` (the degenerate/unset state), the
    /// slider falls back to `min=0.0, max=1.0` rather than dividing by zero.
    pub max: f32,
    /// Quantization step applied to the reported value. `0.0` (the default) means "unset" — continuous, no
    /// snapping, following the same `0.0 == unset` sentinel convention as `width`.
    pub step: f32,
    /// A small caption stacked above the track; omitted entirely (no extra row) when empty.
    pub label: Box<dyn Fn() -> String>,
    /// Fires with the new `min..=max` value on every drag report (the press and each subsequent move).
    pub on_change: Option<Box<dyn Fn(f32)>>,
}

impl Default for SliderProps {
    fn default() -> Self {
        Self {
            value: None,
            color: Box::new(|| Color::TRANSPARENT),
            track_color: Box::new(|| Color::TRANSPARENT),
            width: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.0,
            label: Box::new(String::new),
            on_change: None,
        }
    }
}

pub fn slider(props: SliderProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SliderProps {
        value,
        color,
        track_color,
        width,
        min,
        max,
        step,
        label,
        on_change,
    } = props;
    // Degenerate/unset bounds (max <= min) fall back to the historical 0.0..=1.0 range instead of dividing by zero below.
    let (min, max) = if max <= min { (0.0, 1.0) } else { (min, max) };
    // Uncontrolled: own the value so the slider still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(min));
    let width = if width > 0.0 { width } else { 220.0 };
    // Shared across the fill and thumb style closures (a `Box<dyn Fn>` is not `Clone`, an `Rc` handle is).
    let color: shared::ReactiveColor = Rc::from(color);
    // The drag and the arrow keys are two ways into one commit, so the callback has to reach both.
    let on_change: Option<Rc<dyn Fn(f32)>> = on_change.map(|f| -> Rc<dyn Fn(f32)> { Rc::from(f) });
    let key_on_change = on_change.clone();
    let announced_value = value;
    let commit_value = value;

    // The fill: an `absolute_fill` child (so it exactly overlays the track) scaled horizontally from the left
    // edge by `value` — cheaper than relaying out a narrower box on every drag move.
    let fill_value = value;
    let fill_color = color.clone();
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            let fill = shared::resolve(fill_color.as_ref(), || shared::accent());
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(track_height() / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let t = ((fill_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        // `box_transform` only pivots scale on the rect centre; a progress bar needs the left edge pinned
        // (so it grows rightward from x=0), which needs the raw matrix instead.
        Some(Transform::scale_around(t, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    // The thumb: a normal in-flow child (the only one, since the fill above is out of flow), so it lands at
    // the track's top-left by default; a translate then carries it to `value`'s position and re-centres it
    // vertically against the (thinner) track.
    let thumb_value = value;
    let thumb_color = color.clone();
    let thumb = StyledContainer::new(
        thumb_box(),
        move |_r| {
            let fill = shared::resolve(thumb_color.as_ref(), || shared::accent());
            RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(Color::WHITE, 2.0))
                .with_radius(BorderRadius::all(thumb_size() / 2.0))
        },
        vec![],
    )?
    .styled_by(thumb_box)
    .with_transform(move |r| {
        let t = ((thumb_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        let tx = t * (width - thumb_size());
        let ty = (track_height() - thumb_size()) / 2.0;
        box_transform(r, 0.0, 1.0, 1.0, tx, ty)
    });

    let track = StyledContainer::new(
        track_box(width),
        move |_r| {
            let fill = shared::resolve(track_color.as_ref(), shared::muted);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(track_height() / 2.0))
        },
        vec![box_item(fill), box_item(thumb)],
    )?
    .styled_by(move || track_box(width))
    // A slider you can reach but not move is not operable, so the arrows are half of what makes it a control.
    // One step per press, or a twentieth of the range when the caller named no step — the granularity a
    // continuous value has to invent for a keyboard, which only ever hands it whole presses.
    .control(Role::Slider)
    .valued({
        let value = announced_value;
        move || platform_core::NumericValue {
            now: value.get() as f64,
            min: min as f64,
            max: max as f64,
        }
    })
    .on_key({
        let value = commit_value;
        let on_change = key_on_change.clone();
        move |key: &platform_core::Key| {
            let delta = match key {
                platform_core::Key::Named(platform_core::NamedKey::ArrowRight)
                | platform_core::Key::Named(platform_core::NamedKey::ArrowUp) => 1.0,
                platform_core::Key::Named(platform_core::NamedKey::ArrowLeft)
                | platform_core::Key::Named(platform_core::NamedKey::ArrowDown) => -1.0,
                _ => return,
            };
            let stride = if step > 0.0 { step } else { (max - min) / 20.0 };
            let next = (value.peek() + delta * stride).clamp(min, max);
            value.set(next);
            if let Some(cb) = &on_change {
                cb(next);
            }
        }
    })
    .on_drag(move |px, _py| {
        // `px` is already local to the track (`on_drag` reports widget-local coords), so no rect subtraction here.
        let t = (px / width).clamp(0.0, 1.0);
        let mut v = min + t * (max - min);
        if step > 0.0 {
            // Snap to the nearest step, then re-clamp — rounding can walk a boundary value just past min/max.
            let steps = ((v - min) / step).round();
            v = (min + steps * step).clamp(min, max);
        }
        value.set(v);
        if let Some(cb) = &on_change {
            cb(v);
        }
    });

    shared::captioned(box_item(track), label, width)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use geometry_core::Rect;

    use reactive_core::signal;
    use ui_core::{Component, LayoutItem, NodeId};

    use super::*;
    use crate::harness::{moved, press, release};

    // Lays `node` out inside a 300×100 root and returns its laid-out rect, for computing drag points on the track.
    fn lay_out(node: NodeId) -> Rect {
        crate::harness::lay_out(node, 300.0, 100.0)
    }

    // The core contract: dragging to the track's midpoint maps to value ≈ 0.5, and to the far edge maps to 1.0.
    #[test]
    fn drag_to_midpoint_sets_value_half() {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value),
            width: 200.0,
            ..Default::default()
        })
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

    // An unset `value` prop must fall back to a working internal signal (uncontrolled mode), not panic.
    #[test]
    fn uncontrolled_slider_builds_with_default_value() {
        crate::test_support::fresh_layout_runtime();
        let result = slider(SliderProps::default());
        assert!(result.is_ok());
    }

    // on_change fires with the same mapped value the bound signal receives.
    #[test]
    fn on_change_fires_with_mapped_value() {
        let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
        let sink = seen.clone();
        crate::test_support::fresh_layout_runtime();
        let mut widget = slider(SliderProps {
            width: 100.0,
            on_change: Some(Box::new(move |v| sink.set(v))),
            ..Default::default()
        })
        .unwrap();
        let rect = lay_out(widget.layout_node());

        widget.on_event(&press((rect.x + 25.0) as f64, (rect.y + 4.0) as f64));
        assert!(
            (seen.get() - 0.25).abs() < 1e-4,
            "on_change should see the same mapped value, got {}",
            seen.get()
        );
    }

    // A custom min/max range reports the drag in that range, not normalized 0..1.
    #[test]
    fn custom_range_maps_midpoint_to_range_midpoint() {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value),
            width: 200.0,
            min: 0.0,
            max: 100.0,
            ..Default::default()
        })
        .unwrap();
        let rect = lay_out(widget.layout_node());

        widget.on_event(&press((rect.x + 100.0) as f64, (rect.y + 4.0) as f64));
        assert!(
            (value.get() - 50.0).abs() < 1e-3,
            "midpoint press over 0..100 should map to 50.0, got {}",
            value.get()
        );
    }

    // A non-zero `step` snaps the reported value to the nearest step, not the raw continuous mapping.
    #[test]
    fn step_snaps_value_to_nearest_step() {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value),
            width: 100.0,
            min: 0.0,
            max: 100.0,
            step: 10.0,
            ..Default::default()
        })
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

    // A `label` wraps the track in a labelled column instead of panicking.
    #[test]
    fn label_builds_without_panicking() {
        crate::test_support::fresh_layout_runtime();
        let result = slider(SliderProps {
            label: Box::new(|| "Volume".to_string()),
            ..Default::default()
        });
        assert!(result.is_ok());
    }
}
