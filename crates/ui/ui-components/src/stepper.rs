use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_theme_tokens;
use ui_core::{Container, LayoutItem, StyledContainer, Text, box_item};

use crate::shared;

/// − / + button side length (px) — square, small enough to sit beside the value without dominating it.
fn button_size() -> f32 {
    shared::icon_size() * 1.5
}
/// Gap between the − button, the value, and the + button.
fn gap() -> f32 {
    shared::spacing()
}
fn glyph_size() -> f32 {
    shared::font_size()
}
fn radius() -> f32 {
    shared::radius()
}
fn value_size() -> f32 {
    shared::font_size()
}

/// A numeric stepper: `[−]  value  [+]`. High-level sugar over `button`-style pressable boxes (see
/// `button.rs`) plus a reactive `Text::auto` readout; lives in `ui-components`, not the kernel. `value` is
/// `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own `signal(min)`).
pub struct StepperProps {
    /// Bound value. `None` (the default) is uncontrolled — the widget makes its own `signal(min)`.
    pub value: Option<RwSignal<f32>>,
    /// Lower bound. Default `0.0`.
    pub min: f32,
    /// Upper bound. `0.0` (the default) means "unset". Following the same `0.0 == unset` sentinel convention
    /// as `slider`'s `width`/`step`: an unset (or degenerate, `max <= min`) upper bound falls back to
    /// `f32::INFINITY` rather than the historical two-arg default, so an unset max never pins the value to
    /// `min` (a fixed fallback range would clamp any caller-supplied starting value above it back down).
    pub max: f32,
    /// Increment applied per press. `0.0` (the default) means "unset" — use `1.0`.
    pub step: f32,
    /// − / + fill. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's primary token.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with the new (already clamped) value on every − / + press.
    pub on_change: Option<Box<dyn Fn(f32)>>,
}

impl Default for StepperProps {
    fn default() -> Self {
        Self {
            value: None,
            min: 0.0,
            max: 0.0,
            step: 0.0,
            color: Box::new(|| Color::TRANSPARENT),
            on_change: None,
        }
    }
}

pub fn stepper(props: StepperProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let StepperProps {
        value,
        min,
        max,
        step,
        color,
        on_change,
    } = props;
    let max = if max <= min { f32::INFINITY } else { max };
    let step = if step == 0.0 { 1.0 } else { step };
    // Uncontrolled: own the value so the stepper still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(min));
    // Shared across the − and + buttons' fill closures (a `Box<dyn Fn>` is not `Clone`, an `Rc` handle is).
    let color: shared::ReactiveColor = Rc::from(color);
    // Re-erased to `Rc` so both buttons' `on_press` closures can hold a copy (the field itself is a one-shot `Box`).
    let on_change: Option<Rc<dyn Fn(f32)>> = on_change.map(Rc::from);

    let minus = stepper_button("−", Rc::clone(&color), {
        let value = value.clone();
        let on_change = on_change.clone();
        move || {
            let v = (value.get() - step).clamp(min, max);
            value.set(v);
            if let Some(cb) = &on_change {
                cb(v);
            }
        }
    })?;

    let plus = stepper_button("+", Rc::clone(&color), {
        let value = value.clone();
        let on_change = on_change.clone();
        move || {
            let v = (value.get() + step).clamp(min, max);
            value.set(v);
            if let Some(cb) = &on_change {
                cb(v);
            }
        }
    })?;

    // A measured leaf (`Text::auto`) so the readout has intrinsic width in this row; a stretched
    // `Text::new`/`single_line` would collapse to 0-wide, per `button.rs`'s label.
    let display_value = value.clone();
    let display = Text::auto(
        move || {
            let v = display_value.get();
            if v.fract() == 0.0 {
                format!("{}", v as i64)
            } else {
                format!("{v}")
            }
        },
        LayoutStyle::new(),
        || TextStyle::new(value_size(), shared::ink()),
    )?;

    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(gap()),
        vec![box_item(minus), box_item(display), box_item(plus)],
    )?;
    Ok(box_item(row))
}

/// A small square pressable box with a centred glyph — the − / + buttons, built on the same
/// box + on_press + centred-label shape as `button.rs`'s `ButtonProps` filled variant.
fn stepper_button(
    glyph: &'static str,
    color: shared::ReactiveColor,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let glyph_widget = Text::auto(
        move || glyph.to_string(),
        LayoutStyle::new(),
        // Always the filled variant (never ghost/outline), so the glyph is always white, per `button.rs`'s
        // `label_color` filled case.
        || TextStyle::new(glyph_size(), Color::WHITE),
    )?;
    let container = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .width(button_size())
            .height(button_size()),
        move |_r| {
            let fill = shared::resolve(color.as_ref(), || {
                use_theme_tokens()
                    .map(|t| t.primary())
                    .unwrap_or(shared::DEFAULT_ACCENT)
            });
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(radius()))
        },
        vec![box_item(glyph_widget)],
    )?
    .on_press(on_press);
    Ok(box_item(container))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use ui_core::{Component, compute_layout, track_layout};

    use super::*;

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
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

    // Taps (press then release inside) − and + at their expected edge positions: the row has no padding, so
    // the − button occupies its left button_size() px and the + button its right button_size() px.
    fn tap_minus(widget: &mut Box<dyn LayoutItem>, r: geometry_core::Rect) {
        let (x, y) = (
            (r.x + button_size() / 2.0) as f64,
            (r.y + r.height / 2.0) as f64,
        );
        widget.on_event(&press(x, y));
        widget.on_event(&release(x, y));
    }
    fn tap_plus(widget: &mut Box<dyn LayoutItem>, r: geometry_core::Rect) {
        let (x, y) = (
            (r.x + r.width - button_size() / 2.0) as f64,
            (r.y + r.height / 2.0) as f64,
        );
        widget.on_event(&press(x, y));
        widget.on_event(&release(x, y));
    }

    #[test]
    fn plus_increments_by_step_and_clamps_at_max() {
        reset_layout_runtime();
        let value = signal(4.0f32);
        let mut widget = stepper(StepperProps {
            value: Some(value.clone()),
            min: 0.0,
            max: 5.0,
            step: 1.0,
            ..Default::default()
        })
        .unwrap();
        let node = widget.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        let r = rect.get();

        tap_plus(&mut widget, r);
        assert!((value.get() - 5.0).abs() < 1e-4, "got {}", value.get());

        // Another press must clamp at max, not overshoot to 6.
        tap_plus(&mut widget, r);
        assert!(
            (value.get() - 5.0).abs() < 1e-4,
            "+ must clamp at max, got {}",
            value.get()
        );
    }

    #[test]
    fn minus_decrements_by_step_and_clamps_at_min() {
        reset_layout_runtime();
        let value = signal(1.0f32);
        let mut widget = stepper(StepperProps {
            value: Some(value.clone()),
            min: 0.0,
            max: 5.0,
            step: 1.0,
            ..Default::default()
        })
        .unwrap();
        let node = widget.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        let r = rect.get();

        tap_minus(&mut widget, r);
        assert!((value.get() - 0.0).abs() < 1e-4, "got {}", value.get());

        // Another press must clamp at min, not undershoot to -1.
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
        reset_layout_runtime();
        let mut widget = stepper(StepperProps {
            min: 0.0,
            max: 5.0,
            step: 1.0,
            on_change: Some(Box::new(move |v| sink.set(v))),
            ..Default::default()
        })
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

    // An unset `value` prop must fall back to a working internal signal (uncontrolled mode), not panic.
    #[test]
    fn uncontrolled_stepper_builds_with_default_value() {
        reset_layout_runtime();
        let result = stepper(StepperProps::default());
        assert!(result.is_ok());
    }

    // An unset (sentinel 0.0) max must not pin the value to min: pressing + repeatedly keeps climbing.
    #[test]
    fn unset_max_does_not_clamp_to_min() {
        reset_layout_runtime();
        let value = signal(0.0f32);
        let mut widget = stepper(StepperProps {
            value: Some(value.clone()),
            step: 1.0,
            ..Default::default()
        })
        .unwrap();
        let node = widget.layout_node();
        let rect = track_layout(node).unwrap();
        compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
        let r = rect.get();

        for _ in 0..3 {
            tap_plus(&mut widget, r);
        }
        assert!((value.get() - 3.0).abs() < 1e-4, "got {}", value.get());
    }
}
