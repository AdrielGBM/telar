use std::cell::Cell;
use std::rc::Rc;

use layout_core::{LayoutStyle, NodeId, SizeDimension};
use platform_core::{LocationFormat, receive_location_history};
use reactive_core::signal;
use ui_tree::RenderNode;

use super::*;
use crate::anchor_line::use_anchor_at;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::{relayout_if_dirty, reset_layout_runtime, set_surface_size};
use crate::fixed_layer::FixedLayer;
use crate::layout_item::{LayoutItem, box_item};
use crate::link::reveal_anchor;
use crate::page_anchor::PageAnchor;
use crate::scroll_page::ScrollPage;

const SURFACE: Size = Size {
    width: 800.0,
    height: 600.0,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(x, y, width, height)
}

#[test]
fn a_bar_against_the_top_covers_down_to_its_bottom() {
    assert_eq!(
        covered_edges(SURFACE, &[rect(0.0, 0.0, 800.0, 48.0)]),
        (48.0, 0.0)
    );
}

#[test]
fn a_bar_against_the_bottom_covers_up_to_its_top() {
    assert_eq!(
        covered_edges(SURFACE, &[rect(0.0, 540.0, 800.0, 60.0)]),
        (0.0, 60.0)
    );
}

#[test]
fn the_deepest_bar_along_an_edge_is_the_one_that_counts() {
    let boxes = [
        rect(0.0, 0.0, 800.0, 48.0),
        rect(700.0, 0.0, 100.0, 64.0),
        rect(0.0, 560.0, 800.0, 40.0),
    ];
    assert_eq!(covered_edges(SURFACE, &boxes), (64.0, 40.0));
}

#[test]
fn a_box_clear_of_both_edges_or_against_both_covers_neither() {
    let boxes = [
        rect(0.0, 12.0, 800.0, 48.0),
        rect(0.0, 0.0, 240.0, 600.0),
        rect(300.0, 200.0, 200.0, 200.0),
    ];
    assert_eq!(covered_edges(SURFACE, &boxes), (0.0, 0.0));
}

#[test]
fn an_empty_box_covers_nothing() {
    assert_eq!(
        covered_edges(SURFACE, &[rect(0.0, 0.0, 0.0, 48.0)]),
        (0.0, 0.0)
    );
}

#[test]
fn a_viewport_below_part_of_the_bar_is_covered_by_the_rest_of_it() {
    let full = rect(0.0, 0.0, 800.0, 600.0);
    assert_eq!(
        arrival_within((48.0, 40.0), SURFACE, full),
        Insets::new(48.0, 0.0, 40.0, 0.0)
    );
    let kept_clear = rect(0.0, 24.0, 800.0, 552.0);
    assert_eq!(
        arrival_within((48.0, 40.0), SURFACE, kept_clear),
        Insets::new(24.0, 0.0, 16.0, 0.0)
    );
    let below = rect(0.0, 100.0, 800.0, 400.0);
    assert_eq!(
        arrival_within((48.0, 40.0), SURFACE, below),
        Insets::default()
    );
}

fn block(height: f32) -> Canvas {
    Canvas::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(height),
        |_| RenderNode::Empty,
    )
    .unwrap()
}

struct Built {
    page: ScrollPage,
    bar: NodeId,
    target: NodeId,
}

/// A page with a 48px bar fixed over its top and a 40px one over its bottom, and the anchor `target` between the places `before` and `after`, 1000px each.
fn page_under_bars() -> Built {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let nodes = Rc::new(Cell::new(None));
    let built = nodes.clone();
    let mut page = ScrollPage::new_with(move |_| {
        let top = block(48.0);
        let bar = top.layout_node();
        let gap = Canvas::new(LayoutStyle::new().width(0.0).flex_grow(1.0), |_| {
            RenderNode::Empty
        })?;
        let layer = FixedLayer::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(top), box_item(gap), box_item(block(40.0))],
        )?;
        let target = block(100.0).page_anchor(|| "target");
        built.set(Some((bar, target.layout_node())));
        let content = Container::new(
            LayoutStyle::new().flex_column(),
            vec![
                box_item(layer),
                box_item(block(1000.0).page_anchor(|| "before")),
                box_item(target),
                box_item(block(1000.0).page_anchor(|| "after")),
            ],
        )?;
        Ok(Box::new(content))
    })
    .unwrap();
    page.relayout(SURFACE.width, SURFACE.height);
    relayout_if_dirty();
    let (bar, target) = nodes.get().unwrap();
    Built { page, bar, target }
}

#[test]
fn a_page_takes_its_arrival_margin_from_the_bars_fixed_over_it() {
    let Built { page, .. } = page_under_bars();
    assert_eq!(
        page.viewport().arrival_margin(),
        Insets::new(48.0, 0.0, 40.0, 0.0)
    );
}

#[test]
fn an_anchor_arrived_at_under_the_bar_starts_just_below_it() {
    let Built { page, .. } = page_under_bars();
    assert!(reveal_anchor("target"));
    assert_eq!(page.viewport().peek_offset(), (0.0, 952.0));
}

#[test]
fn the_anchor_arrived_at_is_the_place_under_a_line_level_with_the_bar() {
    let Built { page, .. } = page_under_bars();
    assert!(reveal_anchor("target"));
    let bar = page.viewport().arrival_margin().top;
    assert_eq!(bar, 48.0);
    assert_eq!(
        use_anchor_at(bar).as_deref(),
        Some("target"),
        "`before` ends where `target` starts, level with the line"
    );
    assert_eq!(use_anchor_at(bar - 1.0).as_deref(), Some("before"));
}

#[test]
fn an_address_naming_an_anchor_lands_below_the_bar_once_the_page_is_there() {
    reset_layout_runtime();
    receive_location_history(vec![LocationFormat::root().parse("/#target").unwrap()]);
    let Built { page, .. } = page_under_bars();
    assert_eq!(page.viewport().peek_offset(), (0.0, 952.0));
}

#[test]
fn a_hidden_bar_leaves_the_page_no_margin() {
    let Built { page, bar, .. } = page_under_bars();
    crate::context::set_layout_style(
        bar,
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(48.0)
            .shown(false),
    )
    .unwrap();
    relayout_if_dirty();
    assert_eq!(
        page.viewport().arrival_margin(),
        Insets::new(0.0, 0.0, 40.0, 0.0),
        "the bar left standing still counts"
    );
}

#[test]
fn a_taller_bar_moves_the_margin_with_it() {
    let Built { page, bar, .. } = page_under_bars();
    crate::context::set_layout_style(
        bar,
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(72.0),
    )
    .unwrap();
    relayout_if_dirty();
    assert_eq!(page.viewport().arrival_margin().top, 72.0);
}

#[test]
fn a_declared_margin_takes_the_place_of_the_derived_one() {
    let Built { page, .. } = page_under_bars();
    let top = signal(60.0f32);
    let page = page.arrival_margin(move || Insets::new(top.get(), 0.0, 0.0, 0.0));
    assert_eq!(page.viewport().arrival_margin().top, 60.0);
    top.set(0.0);
    assert_eq!(
        page.viewport().arrival_margin(),
        Insets::default(),
        "and follows what it reads"
    );
    assert!(reveal_anchor("target"));
    assert_eq!(page.viewport().peek_offset(), (0.0, 1000.0));
}

#[test]
fn a_page_without_layers_has_no_margin() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let mut page = ScrollPage::new(Box::new(block(2000.0))).unwrap();
    page.relayout(SURFACE.width, SURFACE.height);
    relayout_if_dirty();
    assert_eq!(page.viewport().arrival_margin(), Insets::default());
}

#[test]
fn a_selection_followed_either_way_is_not_left_under_a_bar() {
    let Built { page, target, .. } = page_under_bars();
    let viewport = page.viewport();
    viewport.scroll_to(0.0, 1020.0);
    viewport.reveal(target, 0.0);
    assert_eq!(
        viewport.peek_offset(),
        (0.0, 952.0),
        "up: below the top bar"
    );
    viewport.scroll_to(0.0, 500.0);
    viewport.reveal(target, 0.0);
    assert_eq!(
        viewport.peek_offset(),
        (0.0, 1100.0 + 40.0 - 600.0),
        "down: above the bottom bar"
    );
}
