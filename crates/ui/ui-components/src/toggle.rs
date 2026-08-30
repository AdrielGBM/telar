use std::rc::Rc;

use layout_core::{AlignItems, LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use telar_macros::Props;
use theme_core::use_theme_tokens;
use ui_core::focus::Role;
use ui_core::{Children, LayoutItem, StyledContainer, box_item, box_transform};

use crate::shared;

/// How far the knob slides between off (left inset) and on (right inset): track 40 − knob 16 − 3px each side.
const KNOB_TRAVEL: f32 = 18.0;

/// The off track is the strongest of the three highlight washes: a switch has to read as a *track*
/// against the surface it sits on, whichever end of the ramp that surface is.
fn off_track() -> Color {
    use_theme_tokens().highlight_high()
}

/// A labelled switch: a 40×22 pill whose knob sits left (off) / right (on) and whose track fills with the
/// accent while on; tapping the row toggles its bound `checked` signal (and fires `on_toggle`). High-level
/// sugar over the primitives (`box` + `on_press` + a reactive fill + a `translate`); lives in `ui-components`,
/// not the kernel. `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget
/// owns its own signal), `Some` is caller-bound.
#[derive(Props)]
pub struct ToggleProps {
    /// Bound on/off state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    #[props(some, into, default)]
    pub checked: Option<RwSignal<bool>>,
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Accent (the on-track fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with the new state on every toggle.
    #[props(some, default)]
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
}

pub fn toggle(props: ToggleProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ToggleProps {
        checked,
        label,
        color,
        on_toggle,
    } = props;
    // Uncontrolled: own the state so the switch still flips when the caller binds no signal.
    let checked = checked.unwrap_or_else(|| signal(false));

    // The knob: a fixed white circle laid out at the left inset, slid right by a transform while on (no reflow).
    let knob_on = checked;
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
    let track_on = checked;
    let track = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(40.0)
            .height(22.0)
            .padding_all(3.0)
            .align_items(AlignItems::CENTER),
        move |_r| {
            let fill = if track_on.get() {
                shared::resolve(&color, shared::accent)
            } else {
                off_track()
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(11.0))
        },
        vec![box_item(knob)],
    )?;

    // The whole row is the tap target (switch + label); a tap flips the bound signal and reports the new state.
    let toggle_on = checked;
    let announced = checked;
    shared::labelled_control(
        box_item(track),
        label,
        Role::Switch,
        move || announced.get(),
        move || {
            let next = !toggle_on.get();
            toggle_on.set(next);
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

    use reactive_core::signal;
    use ui_core::{Component, LayoutItem, NodeId};

    use super::*;
    use crate::harness::{centre, press, release};

    fn lay_out(node: NodeId) -> (f64, f64) {
        centre(crate::harness::lay_out(node, 200.0, 100.0))
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
}
