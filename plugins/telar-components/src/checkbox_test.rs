use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{centre, press, release};
use telar::{Component, LayoutItem, NodeId, signal};

use super::*;

fn lay_out(node: NodeId) -> (f64, f64) {
    centre(telar::testing::lay_out(node, 200.0, 100.0))
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

#[derive(Clone)]
struct Squared {
    item: f32,
    checkboxes: Option<CheckboxTokens>,
}

impl telar::ThemeTokens for Squared {
    fn radius_item(&self) -> f32 {
        self.item
    }
    fn border_width_strong(&self) -> f32 {
        3.0
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(checkboxes) = &self.checkboxes {
            extensions.insert(checkboxes.clone());
        }
    }
}

/// The box and the mark, in paint order.
fn box_and_mark(item: Box<dyn LayoutItem>) -> Vec<(telar::Rect, RectStyle)> {
    telar::testing::lay_out(item.layout_node(), 200.0, 100.0);
    let painted: Vec<_> = telar::ComponentList::new(item)
        .commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } if style.fill.is_some() => {
                Some((*rect, **style))
            }
            _ => None,
        })
        .collect();
    painted
}

fn checked(on: bool) -> CheckboxProps {
    CheckboxProps::props()
        .checked(signal(on))
        .label("Agree")
        .build()
}

fn built(props: CheckboxProps) -> Box<dyn LayoutItem> {
    checkbox(props, Children::default()).unwrap()
}

#[test]
fn the_box_is_an_icon_square_framed_at_the_strong_width_and_rounded_like_an_item() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Squared {
        item: 3.5,
        checkboxes: None,
    });
    let [(rect, style), (mark, mark_style)] = box_and_mark(built(checked(true)))[..] else {
        panic!("a checked box paints its box and its mark");
    };
    let side = crate::metrics::icon_size();
    assert_eq!((rect.width, rect.height), (side, side));
    assert_eq!(style.radius, BorderRadius::all(3.5));
    assert_eq!(style.border.map(|b| b.widths[0]), Some(3.0));
    let inset = crate::metrics::gap_sm();
    assert_eq!(mark.width, side - inset * 2.0);
    assert_eq!(
        mark_style.radius,
        BorderRadius::all(0.0),
        "the mark's corner is concentric with the box's, and the box's is smaller than the inset"
    );
}

#[test]
fn a_theme_with_square_items_draws_square_boxes() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Squared {
        item: 0.0,
        checkboxes: None,
    });
    let (_, style) = box_and_mark(built(checked(false)))[0];
    assert_eq!(style.radius, BorderRadius::all(0.0));
}

#[test]
fn the_checkbox_tokens_win_over_the_roles_and_unset_ones_follow_them() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Squared {
        item: 3.5,
        checkboxes: Some(CheckboxTokens {
            size: Some(22.0),
            radius: Some(11.0),
            border_width: None,
        }),
    });
    let [(rect, style), (mark, mark_style)] = box_and_mark(built(checked(true)))[..] else {
        panic!("a checked box paints its box and its mark");
    };
    assert_eq!((rect.width, rect.height), (22.0, 22.0));
    assert_eq!(style.radius, BorderRadius::all(11.0));
    assert_eq!(style.border.map(|b| b.widths[0]), Some(3.0));
    let inset = crate::metrics::gap_sm();
    assert_eq!(mark.width, 22.0 - inset * 2.0);
    assert_eq!(mark_style.radius, BorderRadius::all(11.0 - inset));
}

#[test]
fn the_box_scales_with_the_control_size_in_force() {
    crate::test_support::fresh_layout_runtime();
    let regular = crate::metrics::icon_size();
    let large =
        telar::provide_control_size(telar::ControlSize::Large, || Ok(built(checked(false))))
            .unwrap();
    let (rect, _) = box_and_mark(Box::new(large))[0];
    assert_eq!(rect.width, regular * telar::ControlSize::Large.scale());
}

#[test]
fn a_style_amendment_reshapes_the_box_checked_and_unchecked() {
    let round: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(9.0)));
    for on in [false, true] {
        crate::test_support::fresh_layout_runtime();
        let mut props = checked(on);
        props.style = Some(round.clone());
        let (_, style) = box_and_mark(built(props))[0];
        assert_eq!(style.radius, BorderRadius::all(9.0), "checked {on}");
    }
}
