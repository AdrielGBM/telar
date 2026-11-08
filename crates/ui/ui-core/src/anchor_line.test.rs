use std::cell::Cell;
use std::rc::Rc;

use layout_core::LayoutStyle;
use reactive_core::{dispose_owner, memo, owner_scope, signal};
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use crate::fixed_layer::FixedLayer;
use crate::layout_item::{LayoutItem, box_item};
use crate::page_anchor::PageAnchor;
use crate::scroll_area::LayoutScrollArea;
use crate::scroll_page::ScrollPage;

const BAR: f32 = 48.0;

fn block(height: f32) -> Canvas {
    Canvas::new(LayoutStyle::new().width(400.0).height(height), |_| {
        RenderNode::Empty
    })
    .unwrap()
}

fn place(name: &'static str, height: f32) -> Box<dyn LayoutItem> {
    box_item(block(height).page_anchor(move || name))
}

fn page_of(items: Vec<Box<dyn LayoutItem>>) -> ScrollPage {
    let mut page = ScrollPage::new(Box::new(Container::column(items).unwrap())).unwrap();
    page.relayout(400.0, 300.0);
    page
}

fn under_bar() -> Option<String> {
    use_anchor_at(BAR).map(|name| name.to_string())
}

/// Three sections one after another: 0–500, 500–1500 and 1500–2000.
fn sections() -> ScrollPage {
    page_of(vec![
        place("top", 500.0),
        place("web", 1000.0),
        place("about", 500.0),
    ])
}

#[test]
fn the_place_under_the_line_is_the_one_whose_box_spans_it() {
    reset_layout_runtime();
    let page = sections();
    let viewport = page.viewport();
    assert_eq!(under_bar().as_deref(), Some("top"));
    viewport.scroll_to(0.0, 451.0);
    assert_eq!(under_bar().as_deref(), Some("top"), "the line is at 499");
    viewport.scroll_to(0.0, 452.0);
    assert_eq!(
        under_bar().as_deref(),
        Some("web"),
        "the line is at 500, where the two meet: the one below it is under it"
    );
    viewport.scroll_to(0.0, 1500.0);
    assert_eq!(under_bar().as_deref(), Some("about"));
}

#[test]
fn nothing_is_under_the_line_without_a_page_or_a_place_spanning_it() {
    reset_layout_runtime();
    assert_eq!(under_bar(), None, "no page holds the primary scroll");
    let page = page_of(vec![
        place("top", 500.0),
        box_item(block(500.0)),
        place("about", 1000.0),
    ]);
    page.viewport().scroll_to(0.0, 600.0);
    assert_eq!(under_bar(), None, "the line crosses a box that is no place");
}

#[test]
fn a_reader_of_the_place_follows_the_scroll_and_changes_only_with_the_place() {
    reset_layout_runtime();
    let page = sections();
    let runs = Rc::new(Cell::new(0));
    let current = {
        let runs = runs.clone();
        let current = memo(under_bar);
        reactive_core::effect(move || {
            current.get();
            runs.set(runs.get() + 1);
        });
        current
    };
    assert_eq!(current.get().as_deref(), Some("top"));
    let viewport = page.viewport();
    viewport.scroll_to(0.0, 100.0);
    viewport.scroll_to(0.0, 200.0);
    assert_eq!(runs.get(), 1, "still the same place");
    viewport.scroll_to(0.0, 700.0);
    assert_eq!(current.get().as_deref(), Some("web"));
    assert_eq!(runs.get(), 2);
}

#[test]
fn of_nested_places_the_innermost_is_under_the_line() {
    reset_layout_runtime();
    let chapter = Container::column(vec![place("intro", 200.0), box_item(block(800.0))])
        .unwrap()
        .page_anchor(|| "web");
    let page = page_of(vec![place("top", 500.0), box_item(chapter)]);
    let viewport = page.viewport();
    viewport.scroll_to(0.0, 500.0);
    assert_eq!(
        under_bar().as_deref(),
        Some("intro"),
        "both start at 500; the shorter is inside the other"
    );
    viewport.scroll_to(0.0, 700.0);
    assert_eq!(under_bar().as_deref(), Some("web"));
}

#[test]
fn a_place_follows_its_name_and_leaves_with_its_box() {
    reset_layout_runtime();
    let name = signal(String::from("simulacion"));
    let scope = owner_scope();
    let named = block(500.0).page_anchor(move || name.get());
    let id = scope.id();
    drop(scope);
    let _page = page_of(vec![box_item(named), box_item(block(1000.0))]);
    assert_eq!(under_bar().as_deref(), Some("simulacion"));
    name.set(String::from("simulation"));
    assert_eq!(under_bar().as_deref(), Some("simulation"));
    dispose_owner(id);
    assert_eq!(under_bar(), None);
}

/// A scroll area 100px tall, 40px down the page, over two places 200px tall each.
#[test]
fn a_place_in_a_scroll_area_counts_only_where_the_area_shows_it() {
    reset_layout_runtime();
    let mut inner_viewport = None;
    let inner =
        LayoutScrollArea::new_with(LayoutStyle::new().width(400.0).height(100.0), |viewport| {
            inner_viewport = Some(viewport);
            Ok(box_item(
                Container::column(vec![place("first", 200.0), place("second", 200.0)]).unwrap(),
            ))
        })
        .unwrap();
    let inner_viewport = inner_viewport.expect("the content was built");
    let page = page_of(vec![
        box_item(block(40.0)),
        box_item(inner),
        box_item(block(1000.0)),
    ]);
    assert_eq!(under_bar().as_deref(), Some("first"));
    inner_viewport.scroll_to(0.0, 195.0);
    assert_eq!(
        under_bar().as_deref(),
        Some("second"),
        "the area shows 195–295 of its content: the first place ends 5px into it, above the line"
    );
    page.viewport().scroll_to(0.0, 100.0);
    assert_eq!(under_bar(), None, "the line, at 148, is below the area");
}

#[test]
fn a_place_in_a_layer_over_the_page_is_never_under_the_line() {
    reset_layout_runtime();
    let layer = FixedLayer::new(LayoutStyle::new(), vec![place("bar", 300.0)]).unwrap();
    let _page = page_of(vec![box_item(layer), box_item(block(1000.0))]);
    assert_eq!(under_bar(), None);
}

/// The scroll area asked, not the page: the line runs across that area's own view.
#[test]
fn any_scroll_answers_for_the_places_in_it() {
    reset_layout_runtime();
    let mut inner_viewport = None;
    let inner =
        LayoutScrollArea::new_with(LayoutStyle::new().width(400.0).height(100.0), |viewport| {
            inner_viewport = Some(viewport);
            Ok(box_item(
                Container::column(vec![place("first", 200.0), place("second", 200.0)]).unwrap(),
            ))
        })
        .unwrap();
    let inner_viewport = inner_viewport.expect("the content was built");
    let outside = block(10.0).page_anchor(|| "outside");
    let _page = page_of(vec![box_item(outside), box_item(inner)]);
    let at = |line| inner_viewport.anchor_at(line).map(|name| name.to_string());
    assert_eq!(at(10.0).as_deref(), Some("first"));
    assert_eq!(
        at(250.0).as_deref(),
        Some("second"),
        "past the view, but in the content"
    );
    inner_viewport.scroll_to(0.0, 150.0);
    assert_eq!(at(60.0).as_deref(), Some("second"));
}
