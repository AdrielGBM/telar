use layout_core::LayoutStyle;
use platform_core::{Location, LocationFormat, anchor, location_history, receive_location_history};
use reactive_core::signal;
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;
use crate::link::{follow, reader_moved};
use crate::scroll_page::ScrollPage;

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

fn block(height: f32) -> Canvas {
    Canvas::new(LayoutStyle::new().width(400.0).height(height), |_| {
        RenderNode::Empty
    })
    .unwrap()
}

/// A page with 1000px above the anchor `name` and 1000px below it, laid out in a 400×300 window.
fn page_with(name: impl Fn() -> String + 'static) -> ScrollPage {
    let column = Container::column(vec![
        box_item(block(1000.0)),
        box_item(block(100.0).page_anchor(name)),
        box_item(block(1000.0)),
    ])
    .unwrap();
    let mut page = ScrollPage::new(Box::new(column)).unwrap();
    page.relayout(400.0, 300.0);
    page
}

fn offset(page: &ScrollPage) -> (f32, f32) {
    page.viewport().peek_offset()
}

#[test]
fn a_link_to_an_anchor_brings_it_to_the_top_and_adds_an_entry() {
    reset_layout_runtime();
    receive_location_history(vec![at("/es")]);
    let page = page_with(|| "contact".into());
    assert!(follow(&anchor("contact")));
    assert_eq!(offset(&page), (0.0, 1000.0));
    assert_eq!(location_history(), [at("/es"), at("/es#contact")]);
    assert!(!follow(&anchor("nowhere")), "no box answers to it");
}

#[test]
fn an_address_naming_an_anchor_reveals_it_once_the_page_is_there() {
    reset_layout_runtime();
    receive_location_history(vec![at("/es#contact")]);
    let page = page_with(|| "contact".into());
    assert_eq!(offset(&page), (0.0, 1000.0));
}

#[test]
fn the_reader_moving_the_page_ends_the_arrival() {
    reset_layout_runtime();
    receive_location_history(vec![at("/#contact")]);
    let mut page = page_with(|| "contact".into());
    reader_moved();
    page.viewport().scroll_to(0.0, 10.0);
    page.relayout(400.0, 280.0);
    assert_eq!(offset(&page), (0.0, 10.0));
}

#[test]
fn an_anchor_name_follows_what_it_reads_and_reaches_the_element() {
    reset_layout_runtime();
    receive_location_history(vec![at("/")]);
    let name = signal(String::from("simulacion"));
    let target = block(10.0).page_anchor(move || name.get());
    let node = target.layout_node();
    let anchor_of = || crate::annotation::peek(node).and_then(|a| a.anchor);
    assert_eq!(anchor_of().as_deref(), Some("simulacion"));
    assert!(crate::link::has_anchor("simulacion"));
    name.set(String::from("simulation"));
    assert_eq!(anchor_of().as_deref(), Some("simulation"));
    assert!(!crate::link::has_anchor("simulacion"));
    assert!(crate::link::has_anchor("simulation"));
    drop(target);
}

#[test]
fn a_renamed_anchor_takes_the_address_and_the_arrival_with_it() {
    reset_layout_runtime();
    receive_location_history(vec![at("/"), at("/#simulacion")]);
    let name = signal(String::from("simulacion"));
    let page = page_with(move || name.get());
    name.set("simulation".into());
    assert_eq!(location_history(), [at("/"), at("/#simulation")]);
    page.viewport().scroll_to(0.0, 0.0);
    crate::link::anchor_moved("simulation");
    assert_eq!(
        offset(&page),
        (0.0, 1000.0),
        "the arrival keeps pulling the page toward the anchor under its new name"
    );
}
