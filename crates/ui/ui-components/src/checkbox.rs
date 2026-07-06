use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use theme_core::use_widget_theme;
use ui_core::{Container, LayoutItem, StyledContainer, Text, WidgetCtx, box_item};

/// Fallback accent when no reactive `color` is supplied and no theme is active (matches `Button`'s default primary).
const DEFAULT_ACCENT: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
/// The unchecked box's fill, its 1.5px border, and the label ink.
const SURFACE: Color = Color::WHITE;
const BORDER: Color = Color::rgba(0.75, 0.77, 0.80, 1.0);
const LABEL: Color = Color::rgba(0.15, 0.15, 0.2, 1.0);

/// A labelled checkbox: an 18px box that fills with the accent and shows a check while its bound `checked`
/// signal is on; tapping the row toggles it (and fires `on_toggle`). High-level sugar over the primitives
/// (`box` + `on_press` + a reactive fill); lives in `ui-components`, not the kernel, so an app can drop it.
/// `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own
/// signal), `Some` is caller-bound.
pub struct CheckboxProps {
    /// Bound checked state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    pub checked: Option<RwSignal<bool>>,
    pub label: &'static str,
    /// Accent (the checked fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with the new state on every toggle.
    pub on_toggle: Option<Box<dyn Fn(bool)>>,
}

impl Default for CheckboxProps {
    fn default() -> Self {
        Self {
            checked: None,
            label: "",
            color: Box::new(|| Color::TRANSPARENT),
            on_toggle: None,
        }
    }
}

pub fn checkbox(
    ctx: &mut WidgetCtx,
    props: CheckboxProps,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CheckboxProps {
        checked,
        label,
        color,
        on_toggle,
    } = props;
    // Uncontrolled: own the state so the box still toggles when the caller binds no signal.
    let checked = checked.unwrap_or_else(|| signal(false));

    // The check: a small inner square that only paints (white) while checked, so toggling never reflows.
    let mark_checked = checked.clone();
    let mark = StyledContainer::new(
        ctx,
        LayoutStyle::new().width(10.0).height(10.0),
        move |_r| {
            let fill = if mark_checked.get() {
                Color::WHITE
            } else {
                Color::TRANSPARENT
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(2.0))
        },
        vec![],
    )?;

    // The 18px box: an accent fill when checked, else a bordered white square, with the check centred inside.
    let box_checked = checked.clone();
    let control = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .width(18.0)
            .height(18.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| {
            let radius = BorderRadius::all(5.0);
            if box_checked.get() {
                RectStyle::default()
                    .with_fill(accent(color.as_ref()))
                    .with_radius(radius)
            } else {
                RectStyle::default()
                    .with_fill(SURFACE)
                    .with_stroke(Stroke::new(BORDER, 1.5))
                    .with_radius(radius)
            }
        },
        vec![box_item(mark)],
    )?;

    let mut children: Vec<Box<dyn LayoutItem>> = vec![box_item(control)];
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

    // The whole row is the tap target (box + label); a tap flips the bound signal and reports the new state.
    let toggle_checked = checked.clone();
    let row = Container::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .align_items(AlignItems::CENTER),
        children,
    )?
    .on_press(move || {
        let next = !toggle_checked.get();
        toggle_checked.set(next);
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
        let checked = signal(false);
        let mut widget = checkbox(
            &mut ctx,
            CheckboxProps {
                checked: Some(checked.clone()),
                label: "Agree",
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert!(checked.get(), "a tap turns the checkbox on");

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert!(!checked.get(), "a second tap turns it back off");
    }

    #[test]
    fn on_toggle_reports_new_state() {
        let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let mut widget = checkbox(
            &mut ctx,
            CheckboxProps {
                on_toggle: Some(Box::new(move |v| sink.set(Some(v)))),
                ..Default::default()
            },
        )
        .unwrap();
        let (cx, cy) = lay_out(&mut ctx, widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(
            seen.get(),
            Some(true),
            "on_toggle fires with the new checked state"
        );
    }
}
