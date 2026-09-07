use super::tests::{make_scroll_area, make_scroll_area_wide};
use super::*;
use crate::canvas::Canvas;
use crate::context::{compute_layout, reset_layout_runtime};
use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};

pub(super) fn press(x: f32, y: f32) -> Event {
    Event::PointerPressed {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

pub(super) fn moved(x: f32, y: f32) -> Event {
    Event::PointerMoved {
        x: x as f64,
        y: y as f64,
        source: PointerSource::Mouse,
    }
}

fn released(x: f32, y: f32) -> Event {
    Event::PointerReleased {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

// Viewport 400x300 over content 400x1000: a 90px thumb with 210px of travel over 700px of scroll.
fn tall() -> ScrollArea {
    make_scroll_area()
}

/// The content moves by its own range, not by the distance the thumb was dragged.
#[test]
fn dragging_the_thumb_moves_the_content_in_proportion() {
    let mut sa = tall();
    assert_eq!(sa.on_event(&press(396.0, 45.0)), EventResult::Handled);
    sa.on_event(&moved(396.0, 150.0));
    assert_eq!(
        sa.core.scroll_y.get(),
        350.0,
        "half the travel, half the range"
    );
    sa.on_event(&moved(396.0, 400.0));
    assert_eq!(sa.core.scroll_y.get(), 700.0, "and it stops at the end");
}

#[test]
fn the_thumb_is_grabbed_where_it_was_taken_hold_of() {
    let mut sa = tall();
    // Held near its lower edge: the thumb must not jump so its top lands under the pointer.
    sa.on_event(&press(396.0, 80.0));
    sa.on_event(&moved(396.0, 80.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0, "no jump on the first move");
}

#[test]
fn a_press_on_the_track_pages_towards_it() {
    let mut sa = tall();
    sa.on_event(&press(396.0, 200.0));
    assert_eq!(sa.core.scroll_y.get(), 300.0, "one viewport down");
    sa.on_event(&released(396.0, 200.0));
    sa.on_event(&press(396.0, 5.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0, "and back up again");
}

#[test]
fn letting_go_ends_the_drag() {
    let mut sa = tall();
    sa.on_event(&press(396.0, 45.0));
    sa.on_event(&moved(396.0, 150.0));
    sa.on_event(&released(396.0, 150.0));
    let settled = sa.core.scroll_y.get();
    sa.on_event(&moved(396.0, 250.0));
    assert_eq!(sa.core.scroll_y.get(), settled, "the pointer is free again");
}

/// The bar is a strip at the edge; a press anywhere else is the content's.
#[test]
fn a_press_beside_the_bar_is_not_a_grab() {
    let mut sa = tall();
    sa.on_event(&press(100.0, 45.0));
    sa.on_event(&moved(100.0, 250.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0);
}

#[test]
fn there_is_nothing_to_grab_when_the_content_fits() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(120.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let node = content.layout_node();
    let mut sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert_ne!(
        sa.on_event(&press(396.0, 45.0)),
        EventResult::Handled,
        "no bar is drawn, so nothing there answers a press"
    );
}

/// Viewport 400x300 over content 1000x300: a 160px thumb with 240px of travel over 600px of scroll.
#[test]
fn the_horizontal_bar_drags_the_same_way() {
    let mut sa = make_scroll_area_wide();
    assert_eq!(sa.on_event(&press(80.0, 296.0)), EventResult::Handled);
    sa.on_event(&moved(200.0, 296.0));
    assert_eq!(sa.core.scroll_x.get(), 300.0);
}
