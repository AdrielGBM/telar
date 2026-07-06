use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{Container, LayoutItem, StyledContainer, Text, WidgetCtx, box_item, box_transform};

/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// The off-track fill and the label ink.
const OFF_TRACK: Color = Color::rgba(0.80, 0.82, 0.85, 1.0);
const LABEL: Color = Color::rgba(0.15, 0.15, 0.2, 1.0);
/// How far the knob slides between off (left inset) and on (right inset): track 40 − knob 16 − 3px each side.
const KNOB_TRAVEL: f32 = 18.0;

/// A labelled switch: a 40×22 pill whose knob sits left (off) / right (on) and whose track fills with the
/// accent while on; tapping the row toggles its bound `checked` signal (and fires `on_toggle`). High-level
/// sugar over the primitives (`box` + `on_press` + a reactive fill + a `translate`); lives in `ui-components`,
/// not the kernel. `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget
/// owns its own signal), `Some` is caller-bound.
pub struct ToggleProps {
    /// Bound on/off state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    pub checked: Option<RwSignal<bool>>,
    pub label: &'static str,
    /// Accent (the on-track fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with the new state on every toggle.
    pub on_toggle: Option<Box<dyn Fn(bool)>>,
}

impl Default for ToggleProps {
    fn default() -> Self {
        Self {
            checked: None,
            label: "",
            color: Box::new(|| Color::TRANSPARENT),
            on_toggle: None,
        }
    }
}

pub fn toggle(ctx: &mut WidgetCtx, props: ToggleProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ToggleProps {
        checked,
        label,
        color,
        on_toggle,
    } = props;
    // Uncontrolled: own the state so the switch still flips when the caller binds no signal.
    let checked = checked.unwrap_or_else(|| signal(false));

    // The knob: a fixed white circle laid out at the left inset, slid right by a transform while on (no reflow).
    let knob_on = checked.clone();
    let knob = StyledContainer::new(
        ctx,
        LayoutStyle::new().width(16.0).height(16.0),
        |_r| {
            RectStyle::default()
                .with_fill(Color::WHITE)
                .with_radius(BorderRadius::all(8.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let tx = if knob_on.get() { KNOB_TRAVEL } else { 0.0 };
        box_transform(r, 0.0, 1.0, 1.0, tx, 0.0)
    });

    // The 40×22 pill: an accent fill when on, a neutral grey when off; the 3px padding insets the knob.
    let track_on = checked.clone();
    let track = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .width(40.0)
            .height(22.0)
            .padding_all(3.0)
            .align_items(AlignItems::CENTER),
        move |_r| {
            let fill = if track_on.get() {
                accent(color.as_ref())
            } else {
                OFF_TRACK
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(11.0))
        },
        vec![box_item(knob)],
    )?;

    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(track)];
    if !label.is_empty() {
        // `auto` (measured leaf) so the label gets its intrinsic WIDTH in this row; a plain `Text::new`
        // only stretches its cross-axis (height here), leaving width 0 and the label invisible.
        let text = Text::auto(
            ctx,
            move || label.to_string(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, LABEL),
        )?;
        children.push(box_item(text));
    }

    // The whole row is the tap target (switch + label); a tap flips the bound signal and reports the new state.
    let toggle_on = checked.clone();
    let row = Container::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .align_items(AlignItems::CENTER),
        children,
    )?
    .on_press(move || {
        let next = !toggle_on.get();
        toggle_on.set(next);
        if let Some(cb) = &on_toggle {
            cb(next);
        }
    });
    Ok(box_item(row))
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

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
    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Lays `node` out inside a 200×100 root and returns its centre point, for tapping.
    fn lay_out(ctx: &mut WidgetCtx, node: NodeId) -> (f64, f64) {
        let rect = track_layout(ctx, node).unwrap();
        let root = new_container(
            ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let r = rect.get();
        ((r.x + r.width / 2.0) as f64, (r.y + r.height / 2.0) as f64)
    }

    #[test]
    fn tap_toggles_bound_signal() {
        let mut ctx = WidgetCtx::new();
        let on = signal(false);
        let mut widget = toggle(
            &mut ctx,
            ToggleProps {
                checked: Some(on.clone()),
                label: "Notifications",
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

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
        let mut ctx = WidgetCtx::new();
        let mut widget = toggle(
            &mut ctx,
            ToggleProps {
                on_toggle: Some(Box::new(move |v| sink.set(Some(v)))),
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(seen.get(), Some(true), "on_toggle fires with the new state");
    }
}
