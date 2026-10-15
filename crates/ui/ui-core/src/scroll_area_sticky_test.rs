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

/// A header that sticks over the rows scrolling under it, each pressable, in the order a document reads them: the header first.
fn header_over_rows(
    header_pressed: std::rc::Rc<std::cell::Cell<u32>>,
    rows_pressed: std::rc::Rc<std::cell::Cell<u32>>,
) -> ui_tree::ComponentList {
    use renderer_core::{Color, RectStyle};
    reset_layout_runtime();
    let header = crate::StyledContainer::new(
        LayoutStyle::new().sticky().inset_top(0.0).height(40.0),
        |_| RectStyle::default().with_fill(Color::WHITE),
        Vec::new(),
    )
    .unwrap()
    .on_press(move || header_pressed.set(header_pressed.get() + 1));
    let rows = crate::StyledContainer::new(
        LayoutStyle::new().height(1000.0),
        |_| RectStyle::default().with_fill(Color::BLACK),
        Vec::new(),
    )
    .unwrap()
    .on_press(move || rows_pressed.set(rows_pressed.get() + 1));
    let content = crate::container::Container::new(
        LayoutStyle::new().flex_column(),
        vec![Box::new(header) as Box<dyn LayoutItem>, Box::new(rows)],
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
    scroll.core.scroll_y.set(500.0);
    crate::context::relayout_if_dirty();
    let tree = ui_tree::ComponentList::new(scroll);
    let _ = tree.commands();
    tree
}

fn tap(tree: &mut ui_tree::ComponentList, x: f64, y: f64) {
    use platform_core::{Event, PointerButton, PointerSource};
    for event in [
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        tree.on_event(&event);
    }
}

/// A box that sticks over the content scrolling under it is drawn over that content, as a document stacks a `position: sticky` box, though it comes first in the order a reader meets them.
#[test]
fn a_stuck_box_is_drawn_over_what_scrolls_under_it() {
    use renderer_core::{Color, DrawCommand};
    let tree = header_over_rows(Default::default(), Default::default());
    let fills: Vec<Color> = tree
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { style, .. } => style.fill.map(|paint| paint.solid_color()),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills[..2],
        [Color::BLACK, Color::WHITE],
        "the rows first, the header over them"
    );
}

/// The pointer meets the boxes in the order they are drawn, so a press on a stuck header is the header's and not the row's under it.
#[test]
fn a_press_on_a_stuck_box_is_its_own_and_not_the_content_s_under_it() {
    let header = std::rc::Rc::new(std::cell::Cell::new(0));
    let rows = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut tree = header_over_rows(header.clone(), rows.clone());
    tap(&mut tree, 150.0, 20.0);
    assert_eq!((header.get(), rows.get()), (1, 0));
    tap(&mut tree, 150.0, 120.0);
    assert_eq!((header.get(), rows.get()), (1, 1));
}
