use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{centre, press, release};
use telar::{BorderRadius, Component, LayoutItem, NodeId, signal};

use super::*;

fn lay_out(node: NodeId) -> (f64, f64) {
    centre(telar::testing::lay_out(node, 200.0, 100.0))
}

#[test]
fn tap_sets_group_selection_to_value() {
    crate::test_support::fresh_layout_runtime();
    let selected = signal(0u32);
    let mut widget = radio(
        RadioProps::props()
            .selected(selected)
            .value(2)
            .label("Large")
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    assert_eq!(selected.get(), 0, "starts unselected (group is on 0)");
    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(selected.get(), 2, "a tap selects this button's value");
}

#[test]
fn on_select_reports_value() {
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    crate::test_support::fresh_layout_runtime();
    let mut widget = radio(
        RadioProps::props()
            .selected(signal(0u32))
            .value(5)
            .on_select(Rc::new(move |v| sink.set(Some(v))))
            .build(),
        Children::default(),
    )
    .unwrap();
    let (cx, cy) = lay_out(widget.layout_node());

    widget.on_event(&press(cx, cy));
    widget.on_event(&release(cx, cy));
    assert_eq!(
        seen.get(),
        Some(5),
        "on_select fires with this button's value"
    );
}

#[derive(Clone, Default)]
struct Plain {
    radios: Option<RadioTokens>,
}

impl telar::ThemeTokens for Plain {
    fn border_width_strong(&self) -> f32 {
        2.5
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(radios) = &self.radios {
            extensions.insert(radios.clone());
        }
    }
}

/// The ring and the dot, in paint order.
fn ring_and_dot(item: Box<dyn LayoutItem>) -> Vec<(telar::Rect, RectStyle)> {
    telar::testing::lay_out(item.layout_node(), 200.0, 100.0);
    telar::ComponentList::new(item)
        .commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } if style.fill.is_some() => {
                Some((*rect, **style))
            }
            _ => None,
        })
        .collect()
}

fn chosen(style: Option<telar::SurfaceAmendment>) -> Box<dyn LayoutItem> {
    let mut props = RadioProps::props()
        .selected(signal(1u32))
        .value(1)
        .label("Small")
        .build();
    props.style = style;
    radio(props, Children::default()).unwrap()
}

#[test]
fn the_ring_and_its_dot_are_pills_framed_at_the_strong_width() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let [(ring, ring_style), (dot, dot_style)] = ring_and_dot(chosen(None))[..] else {
        panic!("a chosen radio paints its ring and its dot");
    };
    let side = crate::metrics::icon_size();
    assert_eq!((ring.width, ring.height), (side, side));
    assert_eq!(ring_style.radius, BorderRadius::all(side / 2.0));
    assert_eq!(ring_style.border.map(|b| b.widths[0]), Some(2.5));
    assert_eq!(dot.width, side - crate::metrics::gap_sm() * 2.0);
    assert_eq!(dot_style.radius, BorderRadius::all(dot.width / 2.0));
}

#[test]
fn the_radio_tokens_win_over_the_roles() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain {
        radios: Some(RadioTokens {
            size: Some(24.0),
            border_width: Some(1.0),
        }),
    });
    let (ring, style) = ring_and_dot(chosen(None))[0];
    assert_eq!((ring.width, ring.height), (24.0, 24.0));
    assert_eq!(style.radius, BorderRadius::all(12.0));
    assert_eq!(style.border.map(|b| b.widths[0]), Some(1.0));
}

#[test]
fn the_ring_scales_with_the_control_size_in_force() {
    crate::test_support::fresh_layout_runtime();
    let regular = crate::metrics::icon_size();
    let large = telar::provide_control_size(telar::ControlSize::Mini, || Ok(chosen(None))).unwrap();
    let (ring, _) = ring_and_dot(Box::new(large))[0];
    assert_eq!(ring.width, regular * telar::ControlSize::Mini.scale());
}

#[test]
fn a_style_amendment_reshapes_the_ring() {
    crate::test_support::fresh_layout_runtime();
    let square: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let [(_, ring), (_, dot)] = ring_and_dot(chosen(Some(square)))[..] else {
        panic!("a chosen radio paints its ring and its dot");
    };
    assert_eq!(ring.radius, BorderRadius::all(0.0));
    assert_ne!(
        dot.radius,
        BorderRadius::all(0.0),
        "the dot is not the principal surface"
    );
}
