use std::rc::Rc;

use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke};
use theme_core::use_widget_theme;
use ui_core::{LayoutItem, StyledContainer, WidgetCtx, box_item, box_transform};

/// Track thickness (px) — a slim pill rail with a floating thumb.
const TRACK_HEIGHT: f32 = 8.0;
/// Thumb diameter (px), bigger than the track so it stays easy to grab; it overhangs the rail on purpose.
const THUMB_SIZE: f32 = 16.0;
/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// Fallback track fill when no theme is active — light enough that the accent fill/thumb still read clearly.
const DEFAULT_TRACK: Color = Color::rgba(0.5, 0.5, 0.6, 0.3);

/// A drag-driven 0.0..=1.0 control: a rounded track, an accent fill up to `value`, and a thumb positioned by
/// it. This is the canonical demo of the `on_drag` primitive (see `StyledContainer::on_drag`): the widget just
/// encapsulates the press/move -> value mapping (and the fill/thumb repaint) an app would otherwise wire up by
/// hand. High-level sugar over the primitives; lives in `ui-components`, not the kernel, so an app can drop it.
/// `value` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own
/// signal), `Some` is caller-bound.
pub struct SliderProps {
    /// Bound value, normalized to 0.0..=1.0 (out-of-range inputs are clamped on every drag report, not here).
    /// `None` (the default) is uncontrolled — the widget makes its own `signal(0.0)`.
    pub value: Option<RwSignal<f32>>,
    /// Fill/thumb accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    pub track_color: Box<dyn Fn() -> Color>,
    /// Track width in px. `0.0` (the default) means "unset" — the slider uses `220.0`.
    pub width: f32,
    /// Fires with the new 0.0..=1.0 value on every drag report (the press and each subsequent move).
    pub on_change: Option<Box<dyn Fn(f32)>>,
}

impl Default for SliderProps {
    fn default() -> Self {
        Self {
            value: None,
            color: Box::new(|| Color::TRANSPARENT),
            track_color: Box::new(|| Color::TRANSPARENT),
            width: 0.0,
            on_change: None,
        }
    }
}

pub fn slider(ctx: &mut WidgetCtx, props: SliderProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SliderProps {
        value,
        color,
        track_color,
        width,
        on_change,
    } = props;
    // Uncontrolled: own the value so the slider still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(0.0));
    let width = if width > 0.0 { width } else { 220.0 };
    // Shared across the fill and thumb style closures (a `Box<dyn Fn>` is not `Clone`, an `Rc` handle is).
    let color: Rc<dyn Fn() -> Color> = Rc::from(color);

    // The fill: an `absolute_fill` child (so it exactly overlays the track) scaled horizontally from the left
    // edge by `value` — cheaper than relaying out a narrower box on every drag move.
    let fill_value = value.clone();
    let fill_color = color.clone();
    let fill = StyledContainer::new(
        ctx,
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            RectStyle::default()
                .with_fill(accent(fill_color.as_ref()))
                .with_radius(BorderRadius::all(TRACK_HEIGHT / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let v = fill_value.get().clamp(0.0, 1.0);
        // `box_transform` only pivots scale on the rect centre; a progress bar needs the left edge pinned
        // (so it grows rightward from x=0), which needs the raw matrix instead.
        Some(Transform::scale_around(v, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    // The thumb: a normal in-flow child (the only one, since the fill above is out of flow), so it lands at
    // the track's top-left by default; a translate then carries it to `value`'s position and re-centres it
    // vertically against the (thinner) track.
    let thumb_value = value.clone();
    let thumb_color = color.clone();
    let thumb = StyledContainer::new(
        ctx,
        LayoutStyle::new().width(THUMB_SIZE).height(THUMB_SIZE),
        move |_r| {
            RectStyle::default()
                .with_fill(accent(thumb_color.as_ref()))
                .with_stroke(Stroke::new(Color::WHITE, 2.0))
                .with_radius(BorderRadius::all(THUMB_SIZE / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let v = thumb_value.get().clamp(0.0, 1.0);
        let tx = v * (width - THUMB_SIZE);
        let ty = (TRACK_HEIGHT - THUMB_SIZE) / 2.0;
        box_transform(r, 0.0, 1.0, 1.0, tx, ty)
    });

    let track = StyledContainer::new(
        ctx,
        LayoutStyle::new().width(width).height(TRACK_HEIGHT),
        move |_r| {
            RectStyle::default()
                .with_fill(track_fill(track_color.as_ref()))
                .with_radius(BorderRadius::all(TRACK_HEIGHT / 2.0))
        },
        vec![box_item(fill), box_item(thumb)],
    )?
    .on_drag(move |px, _py| {
        // `px` is already local to the track (`on_drag` reports widget-local coords), so no rect subtraction here.
        let v = (px / width).clamp(0.0, 1.0);
        value.set(v);
        if let Some(cb) = &on_change {
            cb(v);
        }
    });
    Ok(box_item(track))
}

/// The accent colour: the caller's reactive `color` if set, else the theme's widget primary (as `Button` does).
fn accent(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        use_widget_theme()
            .map(|t| t.widget_primary())
            .unwrap_or(DEFAULT_ACCENT)
    } else {
        c
    }
}

/// The track colour: the caller's reactive `track_color` if set, else the theme's muted token.
fn track_fill(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        use_widget_theme()
            .map(|t| t.widget_muted())
            .unwrap_or(DEFAULT_TRACK)
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use ui_core::{
        Component, LayoutItem, NodeId, WidgetCtx, compute_layout, new_container, track_layout,
    };

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
    fn lay_out(ctx: &mut WidgetCtx, node: NodeId) -> Rect {
        let rect = track_layout(ctx, node).unwrap();
        let root = new_container(
            ctx,
            LayoutStyle::new().flex_column().width(300.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            ctx,
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
        let mut ctx = WidgetCtx::new();
        let value = signal(0.0f32);
        let mut widget = slider(
            &mut ctx,
            SliderProps {
                value: Some(value.clone()),
                width: 200.0,
                ..Default::default()
            },
        )
        .unwrap();
        let rect = lay_out(&mut ctx, widget.layout_node());

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
        let mut ctx = WidgetCtx::new();
        let result = slider(&mut ctx, SliderProps::default());
        assert!(result.is_ok());
    }

    // on_change fires with the same mapped value the bound signal receives.
    #[test]
    fn on_change_fires_with_mapped_value() {
        let seen: Rc<Cell<f32>> = Rc::new(Cell::new(-1.0));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let mut widget = slider(
            &mut ctx,
            SliderProps {
                width: 100.0,
                on_change: Some(Box::new(move |v| sink.set(v))),
                ..Default::default()
            },
        )
        .unwrap();
        let rect = lay_out(&mut ctx, widget.layout_node());

        widget.on_event(&press((rect.x + 25.0) as f64, (rect.y + 4.0) as f64));
        assert!(
            (seen.get() - 0.25).abs() < 1e-4,
            "on_change should see the same mapped value, got {}",
            seen.get()
        );
    }
}
