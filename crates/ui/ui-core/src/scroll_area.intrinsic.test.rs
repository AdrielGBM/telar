use geometry_core::Rect;
use layout_core::{AlignItems, AvailableSpace, LayoutStyle};
use ui_tree::RenderNode;

use super::*;
use crate::Container;
use crate::canvas::Canvas;
use crate::context::{compute_layout, reset_layout_runtime, track_layout};

const CONTENT_WIDTH: f32 = 240.0;
const CONTENT_HEIGHT: f32 = 90.0;

struct Laid {
    viewport: Rect,
    content: Rect,
    offset: (f32, f32),
}

fn content_box(width: f32, height: f32) -> Box<dyn LayoutItem> {
    Box::new(
        Canvas::new(LayoutStyle::new().width(width).height(height), |_| {
            RenderNode::Empty
        })
        .unwrap(),
    )
}

fn lay_out(
    parent: LayoutStyle,
    scroll_style: LayoutStyle,
    content: impl FnOnce() -> Box<dyn LayoutItem>,
) -> Laid {
    reset_layout_runtime();
    let content = content();
    let content_node = content.layout_node();
    let scroll = LayoutScrollArea::new(scroll_style, content).unwrap();
    let (viewport, scroll_x, scroll_y) =
        (scroll.leaf.rect, scroll.core.scroll_x, scroll.core.scroll_y);
    scroll_x.set(1000.0);
    scroll_y.set(1000.0);
    let parent = Container::new(parent, vec![Box::new(scroll) as Box<dyn LayoutItem>]).unwrap();
    compute_layout(
        parent.layout_node(),
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .unwrap();
    Laid {
        viewport: viewport.get(),
        content: track_layout(content_node).unwrap().get(),
        offset: (scroll_x.get(), scroll_y.get()),
    }
}

fn shrink_to_fit() -> LayoutStyle {
    LayoutStyle::new().flex_row().align_items(AlignItems::START)
}

#[test]
fn in_a_shrink_to_fit_parent_the_viewport_is_its_content_size() {
    let laid = lay_out(shrink_to_fit(), LayoutStyle::new(), || {
        content_box(CONTENT_WIDTH, CONTENT_HEIGHT)
    });
    assert_eq!(
        laid.viewport,
        Rect::new(0.0, 0.0, CONTENT_WIDTH, CONTENT_HEIGHT),
        "a scroll area with nothing to bound it is as big as what it shows"
    );
    assert_eq!(
        laid.content,
        Rect::new(0.0, 0.0, CONTENT_WIDTH, CONTENT_HEIGHT),
        "the content is placed in its own space, top-left at the origin"
    );
}

#[test]
fn a_fixed_parent_bounds_the_viewport_and_the_content_keeps_its_size() {
    let laid = lay_out(
        LayoutStyle::new().flex_column().width(300.0).height(100.0),
        LayoutStyle::new().flex_grow(1.0),
        || content_box(CONTENT_WIDTH, 1000.0),
    );
    assert_eq!(
        laid.viewport,
        Rect::new(0.0, 0.0, 300.0, 100.0),
        "a scroll area may shrink below its content, as an overflow box does"
    );
    assert_eq!(laid.content.height, 1000.0);
}

#[test]
fn content_of_auto_width_fills_the_viewport_width() {
    let laid = lay_out(
        LayoutStyle::new().flex_column().width(300.0).height(100.0),
        LayoutStyle::new().flex_grow(1.0),
        || {
            Box::new(
                Container::new(
                    LayoutStyle::new().flex_column(),
                    vec![content_box(50.0, 400.0)],
                )
                .unwrap(),
            )
        },
    );
    assert_eq!(laid.content, Rect::new(0.0, 0.0, 300.0, 400.0));
}

#[test]
fn a_max_height_clamps_the_viewport_and_leaves_the_rest_to_scroll() {
    let laid = lay_out(shrink_to_fit(), LayoutStyle::new().max_height(50.0), || {
        content_box(CONTENT_WIDTH, CONTENT_HEIGHT)
    });
    assert_eq!(laid.viewport, Rect::new(0.0, 0.0, CONTENT_WIDTH, 50.0));
    assert_eq!(laid.content.height, CONTENT_HEIGHT);
    assert!(
        overflows(laid.viewport, laid.content),
        "what the clamp cut off is there to scroll to"
    );
}

#[test]
fn clamped_on_both_axes_it_scrolls_on_both() {
    let laid = lay_out(
        shrink_to_fit(),
        LayoutStyle::new().max_width(100.0).max_height(50.0),
        || content_box(CONTENT_WIDTH, CONTENT_HEIGHT),
    );
    assert_eq!(laid.viewport, Rect::new(0.0, 0.0, 100.0, 50.0));
    assert_eq!(
        laid.content,
        Rect::new(0.0, 0.0, CONTENT_WIDTH, CONTENT_HEIGHT)
    );

    assert_eq!(
        laid.offset,
        (CONTENT_WIDTH - 100.0, CONTENT_HEIGHT - 50.0),
        "the view reaches the far corner of the content and no further"
    );
}

#[test]
fn content_that_grows_grows_the_viewport_it_sizes() {
    reset_layout_runtime();
    let item = Canvas::new(LayoutStyle::new().width(CONTENT_WIDTH).height(40.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let item_node = item.layout_node();
    let scroll = LayoutScrollArea::new(LayoutStyle::new(), Box::new(item)).unwrap();
    let viewport = scroll.leaf.rect;
    let parent = Container::new(
        shrink_to_fit(),
        vec![Box::new(scroll) as Box<dyn LayoutItem>],
    )
    .unwrap();
    compute_layout(
        parent.layout_node(),
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .unwrap();
    assert_eq!(viewport.get().height, 40.0);

    crate::context::set_layout_style(
        item_node,
        LayoutStyle::new().width(CONTENT_WIDTH).height(70.0),
    )
    .unwrap();
    crate::context::relayout_if_dirty();

    assert_eq!(
        viewport.get().height,
        70.0,
        "a change inside the content reaches the box it sizes without the viewport asking"
    );
}
