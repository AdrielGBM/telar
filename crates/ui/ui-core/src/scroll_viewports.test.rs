use layout_core::LayoutStyle;
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::context::reset_layout_runtime;
use crate::layout_item::{LayoutItem, box_item};
use crate::scroll_area::LayoutScrollArea;
use crate::scroll_page::ScrollPage;

fn block() -> Canvas {
    Canvas::new(LayoutStyle::new().width(10.0).height(10.0), |_| {
        RenderNode::Empty
    })
    .unwrap()
}

#[test]
fn a_node_finds_every_scroll_it_is_in_whatever_order_they_were_built() {
    reset_layout_runtime();
    let inner_item = block();
    let item = inner_item.layout_node();
    let inner = LayoutScrollArea::new(LayoutStyle::new(), box_item(inner_item)).unwrap();
    let inner_area = inner.layout_node();
    let page = ScrollPage::new(box_item(inner)).unwrap();
    let chain: Vec<_> = scroll_viewports_of(item).iter().map(|v| v.area()).collect();
    assert_eq!(chain, [inner_area, page.viewport().area()]);
    assert!(enclosing_scroll_viewport(page.viewport().area()).is_none());
}
