use layout_core::{AvailableSpace, LayoutStyle, SizeDimension};
use reactive_core::signal;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, new_container, new_leaf, reset_layout_runtime};
use crate::layout_item::LayoutItem;

fn reset() {
    REGIONS.with(|regions| regions.borrow_mut().clear());
}

/// A trigger inside a scrolled subtree must report where it is *drawn*, not where it was laid out — the whole reason an anchored dropdown was landing under the wrong place.
#[test]
fn a_scrolled_node_reports_its_on_screen_position() {
    reset_layout_runtime();
    reset();
    let (trigger, _r) = new_leaf(LayoutStyle::new().width(50.0).height(20.0)).unwrap();
    let content = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        &[trigger],
    )
    .unwrap();
    compute_layout(
        content,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    let laid_out = absolute_rect(trigger).unwrap();
    assert_eq!(
        visible_rect(trigger),
        Some(laid_out),
        "unscrolled: identical"
    );

    let (x, y) = (signal(0.0f32), signal(120.0f32));
    let id = register_scroll_region(content, x, y);
    let shifted = visible_rect(trigger).unwrap();
    assert_eq!(
        shifted.y,
        laid_out.y - 120.0,
        "scrolled down 120px, so it is drawn 120px higher"
    );
    assert_eq!(shifted.x, laid_out.x);
    assert_eq!(
        shifted.width, laid_out.width,
        "scrolling moves a node, it does not resize it"
    );

    y.set(0.0);
    assert_eq!(visible_rect(trigger), Some(laid_out));
    y.set(80.0);
    unregister_scroll_region(id);
    assert_eq!(visible_rect(trigger), Some(laid_out));
}

/// Nested scrolls compose: each contributes its own offset, without either knowing about the other.
#[test]
fn nested_scroll_offsets_accumulate() {
    reset_layout_runtime();
    reset();
    let inner_leaf = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let trigger = inner_leaf.layout_node();
    let inner = new_container(LayoutStyle::new().flex_column(), &[trigger]).unwrap();
    let outer = new_container(LayoutStyle::new().flex_column(), &[inner]).unwrap();
    compute_layout(
        outer,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();
    let laid_out = absolute_rect(trigger).unwrap();

    register_scroll_region(outer, signal(0.0), signal(30.0));
    register_scroll_region(inner, signal(5.0), signal(7.0));
    let shifted = visible_rect(trigger).unwrap();
    assert_eq!(shifted.y, laid_out.y - 37.0);
    assert_eq!(shifted.x, laid_out.x - 5.0);
}

/// A node outside a registered subtree is untouched by that scroll — otherwise every overlay in the app would shift whenever any unrelated pane scrolled.
#[test]
fn a_node_outside_the_region_is_unaffected() {
    reset_layout_runtime();
    reset();
    let (inside, _a) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
    let (outside, _b) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
    let scrolled = new_container(LayoutStyle::new().flex_column(), &[inside]).unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[scrolled, outside]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let outside_before = absolute_rect(outside).unwrap();
    register_scroll_region(scrolled, signal(0.0), signal(50.0));
    assert_eq!(visible_rect(outside), Some(outside_before));
    assert_eq!(
        visible_rect(inside).unwrap().y,
        absolute_rect(inside).unwrap().y - 50.0
    );
}
