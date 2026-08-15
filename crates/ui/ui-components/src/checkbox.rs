use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke};
use theme_core::use_theme_tokens;
use ui_core::focus::Role;
use ui_core::{LayoutItem, StyledContainer, box_item};

use crate::shared;

/// The unchecked box's fill and its 1.5px border.
const SURFACE: Color = Color::WHITE;
const BORDER: Color = Color::rgba(0.75, 0.77, 0.80, 1.0);

/// A labelled checkbox: an 18px box that fills with the accent and shows a check while its bound `checked`
/// signal is on; tapping the row toggles it (and fires `on_toggle`). High-level sugar over the primitives
/// (`box` + `on_press` + a reactive fill); lives in `ui-components`, not the kernel, so an app can drop it.
/// `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own
/// signal), `Some` is caller-bound.
pub struct CheckboxProps {
    /// Bound checked state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    pub checked: Option<RwSignal<bool>>,
    pub label: Box<dyn Fn() -> String>,
    /// Accent (the checked fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with the new state on every toggle.
    pub on_toggle: Option<Box<dyn Fn(bool)>>,
}

impl Default for CheckboxProps {
    fn default() -> Self {
        Self {
            checked: None,
            label: Box::new(String::new),
            color: Box::new(|| Color::TRANSPARENT),
            on_toggle: None,
        }
    }
}

pub fn checkbox(props: CheckboxProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
        LayoutStyle::new()
            .flex_row()
            .width(18.0)
            .height(18.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| {
            let radius = BorderRadius::all(5.0);
            if box_checked.get() {
                let fill = shared::resolve(color.as_ref(), || {
                    use_theme_tokens()
                        .map(|t| t.primary())
                        .unwrap_or(shared::DEFAULT_ACCENT)
                });
                RectStyle::default().with_fill(fill).with_radius(radius)
            } else {
                RectStyle::default()
                    .with_fill(SURFACE)
                    .with_stroke(Stroke::new(BORDER, 1.5))
                    .with_radius(radius)
            }
        },
        vec![box_item(mark)],
    )?;

    // The whole row is the tap target (box + label); a tap flips the bound signal and reports the new state.
    let toggle_checked = checked.clone();
    let announced = checked.clone();
    shared::labelled_control(
        box_item(control),
        label,
        Role::CheckBox,
        move || announced.get(),
        move || {
            let next = !toggle_checked.get();
            toggle_checked.set(next);
            if let Some(cb) = &on_toggle {
                cb(next);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_core::reset_layout_runtime;

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
    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Lays `node` out inside a 200×100 root and returns its centre point, for tapping.
    fn lay_out(node: NodeId) -> (f64, f64) {
        let rect = track_layout(node).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
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
        reset_layout_runtime();
        let checked = signal(false);
        let mut widget = checkbox(CheckboxProps {
            checked: Some(checked.clone()),
            label: Box::new(|| "Agree".to_string()),
            ..Default::default()
        })
        .unwrap();
        let (cx, cy) = lay_out(widget.layout_node());

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
        reset_layout_runtime();
        let mut widget = checkbox(CheckboxProps {
            on_toggle: Some(Box::new(move |v| sink.set(Some(v)))),
            ..Default::default()
        })
        .unwrap();
        let (cx, cy) = lay_out(widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(
            seen.get(),
            Some(true),
            "on_toggle fires with the new checked state"
        );
    }
}
