use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, StyledContainer, box_item, box_transform};

use crate::shared;

/// The off-track fill.
const OFF_TRACK: Color = Color::rgba(0.80, 0.82, 0.85, 1.0);
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
    pub label: Box<dyn Fn() -> String>,
    /// Accent (the on-track fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Fires with the new state on every toggle.
    pub on_toggle: Option<Box<dyn Fn(bool)>>,
}

impl Default for ToggleProps {
    fn default() -> Self {
        Self {
            checked: None,
            label: Box::new(String::new),
            color: Box::new(|| Color::TRANSPARENT),
            on_toggle: None,
        }
    }
}

pub fn toggle(props: ToggleProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
        LayoutStyle::new()
            .flex_row()
            .width(40.0)
            .height(22.0)
            .padding_all(3.0)
            .align_items(AlignItems::CENTER),
        move |_r| {
            let fill = if track_on.get() {
                shared::resolve(color.as_ref(), || {
                    use_theme_tokens()
                        .map(|t| t.primary())
                        .unwrap_or(shared::DEFAULT_ACCENT)
                })
            } else {
                OFF_TRACK
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(11.0))
        },
        vec![box_item(knob)],
    )?;

    // The whole row is the tap target (switch + label); a tap flips the bound signal and reports the new state.
    let toggle_on = checked.clone();
    shared::labelled_control(box_item(track), label, move || {
        let next = !toggle_on.get();
        toggle_on.set(next);
        if let Some(cb) = &on_toggle {
            cb(next);
        }
    })
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
        let on = signal(false);
        let mut widget = toggle(ToggleProps {
            checked: Some(on.clone()),
            label: Box::new(|| "Notifications".to_string()),
            ..Default::default()
        })
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
        reset_layout_runtime();
        let mut widget = toggle(ToggleProps {
            on_toggle: Some(Box::new(move |v| sink.set(Some(v)))),
            ..Default::default()
        })
        .unwrap();
        let (cx, cy) = lay_out(widget.layout_node());

        widget.on_event(&press(cx, cy));
        widget.on_event(&release(cx, cy));
        assert_eq!(seen.get(), Some(true), "on_toggle fires with the new state");
    }
}
