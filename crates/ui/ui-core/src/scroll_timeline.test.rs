use layout_core::LayoutStyle;
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;
use crate::scroll_page::ScrollPage;

const VIEW: f32 = 300.0;

#[test]
fn cover_runs_from_entering_the_view_to_leaving_it() {
    let at = |offset| range_progress(ViewRange::Cover, 1000.0, 100.0, VIEW, offset);
    assert_eq!(at(0.0), 0.0);
    assert_eq!(at(700.0), 0.0, "its top reaches the bottom of the view");
    assert_eq!(at(900.0), 0.5);
    assert_eq!(at(1100.0), 1.0, "its bottom leaves the top of the view");
}

#[test]
fn contain_runs_while_a_short_box_is_wholly_in_view() {
    let at = |offset| range_progress(ViewRange::Contain, 1000.0, 100.0, VIEW, offset);
    assert_eq!(at(800.0), 0.0, "its bottom reaches the bottom of the view");
    assert_eq!(at(900.0), 0.5);
    assert_eq!(at(1000.0), 1.0, "its top reaches the top of the view");
}

#[test]
fn contain_runs_while_a_tall_track_fills_the_view() {
    let at = |offset| range_progress(ViewRange::Contain, 1000.0, 1200.0, VIEW, offset);
    assert_eq!(at(1000.0), 0.0, "the track's top meets the view's top");
    assert_eq!(at(1450.0), 0.5);
    assert_eq!(
        at(1900.0),
        1.0,
        "the track's bottom meets the view's bottom"
    );
}

#[test]
fn entry_and_exit_split_the_passage_around_contain() {
    let entry = |offset| range_progress(ViewRange::Entry, 1000.0, 100.0, VIEW, offset);
    let exit = |offset| range_progress(ViewRange::Exit, 1000.0, 100.0, VIEW, offset);
    assert_eq!(entry(700.0), 0.0);
    assert_eq!(entry(750.0), 0.5);
    assert_eq!(entry(800.0), 1.0);
    assert_eq!(exit(1000.0), 0.0);
    assert_eq!(exit(1050.0), 0.5);
    assert_eq!(exit(1100.0), 1.0);
}

#[test]
fn a_range_of_no_length_is_a_step() {
    let at = |offset| range_progress(ViewRange::Contain, 1000.0, VIEW, VIEW, offset);
    assert_eq!(at(999.0), 0.0);
    assert_eq!(at(1000.0), 1.0);
}

#[test]
fn the_words_rsx_spells_are_the_ranges() {
    assert_eq!(ViewRange::parse("contain"), Some(ViewRange::Contain));
    assert_eq!(ViewRange::parse("sideways"), None);
}

fn block(height: f32) -> Canvas {
    Canvas::new(LayoutStyle::new().width(400.0).height(height), |_| {
        RenderNode::Empty
    })
    .unwrap()
}

#[test]
fn a_box_on_the_page_follows_the_page_scrolling() {
    reset_layout_runtime();
    let track = block(1200.0);
    let node = track.layout_node();
    let column = Container::column(vec![
        box_item(block(1000.0)),
        box_item(track),
        box_item(block(1000.0)),
    ])
    .unwrap();
    let mut page = ScrollPage::new(Box::new(column)).unwrap();
    let progress = scroll_progress(node, ViewRange::Contain);
    page.relayout(400.0, VIEW);
    assert_eq!(progress.get(), 0.0);
    let viewport = page.viewport();
    viewport.scroll_to(0.0, 1450.0);
    assert_eq!(progress.get(), 0.5);
    viewport.scroll_to(0.0, 3000.0);
    assert_eq!(progress.get(), 1.0);
    assert!(
        (viewport.progress(Axis::Vertical) - 1.0).abs() < f32::EPSILON,
        "the page as a whole is scrolled to its end, clamped"
    );
}

#[test]
fn content_built_inside_a_scroll_finds_it_by_context() {
    reset_layout_runtime();
    assert!(use_scroll_viewport().is_none());
    let mut found = None;
    let page = ScrollPage::new_with(|_| {
        found = use_scroll_viewport().map(|viewport| viewport.area());
        Ok(box_item(block(10.0)))
    })
    .unwrap();
    assert_eq!(found, Some(page.viewport().area()));
    assert!(
        use_scroll_viewport().is_none(),
        "the context ends with the content"
    );
}
