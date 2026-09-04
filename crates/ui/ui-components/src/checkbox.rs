//! [`checkbox`]: a box and its label, toggling a bound boolean.

use std::rc::Rc;

use telar_macros::Props;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use ui_core::focus::Role;
use ui_core::{Children, LayoutItem, StyledContainer, box_item};

use crate::shared;

/// A labelled checkbox: an 18px box that fills with the accent and shows a check while its bound `checked` signal is on; tapping the row toggles it (and fires `on_toggle`). High-level sugar over the primitives (`box` + `on_press` + a reactive fill); lives in `ui-components`, not the kernel, so an app can drop it. `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own signal), `Some` is caller-bound.
#[derive(Props)]
pub struct CheckboxProps {
    /// Bound checked state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    #[props(some, into, default)]
    pub checked: Option<RwSignal<bool>>,
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Accent (the checked fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with the new state on every toggle.
    #[props(some, default)]
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
}

/// A box and its label, toggling a bound boolean.
pub fn checkbox(
    props: CheckboxProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CheckboxProps {
        checked,
        label,
        color,
        on_toggle,
    } = props;
    // Uncontrolled: own the state so the box still toggles when the caller binds no signal.
    let checked = checked.unwrap_or_else(|| signal(false));

    // Shared between the box's fill and the mark, which reads it to pick an ink that contrasts with it.

    // The check: a small inner square that only paints while checked, so toggling never reflows. Its ink is read off the box's own fill — a hard white vanished on the near-white `primary` of a neutral palette.
    let mark_checked = checked;
    let mark_color = color.clone();
    let mark = StyledContainer::new(
        LayoutStyle::new().width(10.0).height(10.0),
        move |_r| {
            let fill = if mark_checked.get() {
                shared::ink_on(shared::resolve(&mark_color, shared::accent))
            } else {
                Color::TRANSPARENT
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(2.0))
        },
        vec![],
    )?;

    let box_checked = checked;
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
                let fill = shared::resolve(&color, shared::accent);
                RectStyle::default().with_fill(fill).with_radius(radius)
            } else {
                RectStyle::default()
                    .with_fill(shared::surface())
                    .with_border(Border::uniform(shared::border(), 1.5))
                    .with_radius(radius)
            }
        },
        vec![box_item(mark)],
    )?;

    let toggle_checked = checked;
    let announced = checked;
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
        let checked = signal(false);
        let mut widget = checkbox(
            CheckboxProps::props()
                .checked(checked)
                .label("Agree")
                .build(),
            Children::default(),
        )
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
        crate::test_support::fresh_layout_runtime();
        let mut widget = checkbox(
            CheckboxProps::props()
                .on_toggle(Rc::new(move |v| sink.set(Some(v))))
                .build(),
            Children::default(),
        )
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
