use std::rc::Rc;

use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{Container, LayoutItem, StyledContainer, Text, box_item, box_transform};

use crate::shared;

/// Track thickness (px) — a slim pill rail with a floating thumb.
const TRACK_HEIGHT: f32 = 8.0;
/// Thumb diameter (px), bigger than the track so it stays easy to grab; it overhangs the rail on purpose.
const THUMB_SIZE: f32 = 16.0;
/// Caption size/gap above the track, mirroring `text_field`'s label row.
const LABEL_SIZE: f32 = 13.0;
const LABEL_GAP: f32 = 6.0;

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

    // The fill: an `absolute_fill` child (so it exactly overlays the track) scaled horizontally from the left
    // edge by `value` — cheaper than relaying out a narrower box on every drag move.
    let fill_value = value.clone();
    let fill_color = color.clone();
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            let fill = shared::resolve(fill_color.as_ref(), || {
                use_theme_tokens()
                    .map(|t| t.primary())
                    .unwrap_or(shared::DEFAULT_ACCENT)
            });
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(TRACK_HEIGHT / 2.0))
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
    let thumb_value = value.clone();
    let thumb_color = color.clone();
    let thumb = StyledContainer::new(
        LayoutStyle::new().width(THUMB_SIZE).height(THUMB_SIZE),
        move |_r| {
            let fill = shared::resolve(thumb_color.as_ref(), || {
                use_theme_tokens()
                    .map(|t| t.primary())
                    .unwrap_or(shared::DEFAULT_ACCENT)
            });
            RectStyle::default()
                .with_fill(fill)
                .with_stroke(Stroke::new(Color::WHITE, 2.0))
                .with_radius(BorderRadius::all(THUMB_SIZE / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let t = ((thumb_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        let tx = t * (width - THUMB_SIZE);
        let ty = (TRACK_HEIGHT - THUMB_SIZE) / 2.0;
        box_transform(r, 0.0, 1.0, 1.0, tx, ty)
    });

    let track = StyledContainer::new(
        LayoutStyle::new().width(width).height(TRACK_HEIGHT),
        move |_r| {
            let fill = shared::resolve(track_color.as_ref(), || {
                use_theme_tokens()
                    .map(|t| t.muted())
                    .unwrap_or(shared::DEFAULT_TRACK)
            });
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(TRACK_HEIGHT / 2.0))
        },
        vec![box_item(fill), box_item(thumb)],
    )?
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

    if label().is_empty() {
        return Ok(box_item(track));
    }
    let caption = Text::new(
        move || label(),
        LayoutStyle::new().height(LABEL_SIZE * 1.4),
        || TextStyle::new(LABEL_SIZE, label_color()),
    )?;
    let col = Container::new(
        LayoutStyle::new().flex_column().gap(LABEL_GAP).width(width),
        vec![box_item(caption), box_item(track)],
    )?;
    Ok(box_item(col))
}

/// The muted label ink, re-read every frame so it tracks the active theme (mirrors `text_field`'s caption colour).
fn label_color() -> Color {
    use_theme_tokens()
        .map(|t| t.muted())
        .unwrap_or(Color::rgba(0.5, 0.5, 0.6, 0.6))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use ui_core::{Component, LayoutItem, NodeId, compute_layout, new_container, track_layout};

    use super::*;

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }
    fn moved(x: f64, y: f64) -> Event {
        Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        }
    }
    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Lays `node` out inside a 300×100 root and returns its laid-out rect, for computing drag points on the track.
    fn lay_out(node: NodeId) -> Rect {
        let rect = track_layout(node).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(300.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        rect.get()
    }

    // The core contract: dragging to the track's midpoint maps to value ≈ 0.5, and to the far edge maps to 1.0.
    #[test]
    fn drag_to_midpoint_sets_value_half() {
        reset_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value.clone()),
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
        reset_layout_runtime();
        let result = slider(SliderProps::default());
        assert!(result.is_ok());
    }

    // on_change fires with the same mapped value the bound signal receives.
    #[test]
    fn on_change_fires_with_mapped_value() {
        let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
        let sink = seen.clone();
        reset_layout_runtime();
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
        reset_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value.clone()),
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
        reset_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = slider(SliderProps {
            value: Some(value.clone()),
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
        reset_layout_runtime();
        let result = slider(SliderProps {
            label: Box::new(|| "Volume".to_string()),
            ..Default::default()
        });
        assert!(result.is_ok());
    }
}
