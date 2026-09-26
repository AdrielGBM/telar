use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutStyle, NodeId};
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::context::{compute_layout, reset_layout_runtime, track_layout};
use crate::layout_item::LayoutItem;

fn block(style: LayoutStyle) -> Canvas {
    Canvas::new(style, |_| RenderNode::Empty).unwrap()
}

/// A 300×200 viewport over a 100px header, a 40px bar sticking to the top, and 1000px more.
fn page() -> (LayoutScrollArea, NodeId) {
    reset_layout_runtime();
    let bar = block(LayoutStyle::new().sticky().inset_top(0.0).height(40.0));
    let bar_node = bar.layout_node();
    let content = crate::container::Container::new(
        LayoutStyle::new().flex_column(),
        vec![
            Box::new(block(LayoutStyle::new().height(100.0))) as Box<dyn LayoutItem>,
            Box::new(bar),
            Box::new(block(LayoutStyle::new().height(1000.0))),
        ],
    )
    .unwrap();
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().width(300.0).height(200.0),
        Box::new(content),
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    (scroll, bar_node)
}

#[test]
fn a_sticky_box_rests_in_the_flow_until_the_viewport_reaches_it() {
    let (scroll, bar) = page();
    scroll.core.scroll_y.set(60.0);
    assert_eq!(track_layout(bar).unwrap().get().y, 100.0);
}

#[test]
fn scrolling_past_a_sticky_box_keeps_it_at_the_viewports_edge() {
    let (scroll, bar) = page();
    scroll.core.scroll_y.set(500.0);
    assert_eq!(
        track_layout(bar).unwrap().get(),
        Rect::new(0.0, 500.0, 300.0, 40.0),
        "the rect every backend draws from is the stuck one"
    );
    scroll.core.scroll_y.set(0.0);
    assert_eq!(
        track_layout(bar).unwrap().get().y,
        100.0,
        "and it comes back"
    );
}

/// Hit-testing reads the same rect, so a press lands on the box where it is drawn rather than where layout first put it.
#[test]
fn a_stuck_box_is_found_where_it_is_drawn() {
    let (scroll, bar) = page();
    scroll.core.scroll_y.set(500.0);
    let shown = crate::input_region::visible_rect(bar).expect("the bar is on screen");
    assert_eq!((shown.x, shown.y, shown.height), (0.0, 0.0, 40.0));
}
