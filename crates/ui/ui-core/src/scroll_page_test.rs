use layout_core::LayoutStyle;
use renderer_core::Element;
use std::sync::Arc;

use super::*;
use crate::canvas::Canvas;
use crate::context::reset_layout_runtime;

struct AsADocument(bool);

impl AsADocument {
    fn new() -> Self {
        Self(ui_tree::set_element_capture(true))
    }
}

impl Drop for AsADocument {
    fn drop(&mut self) {
        ui_tree::set_element_capture(self.0);
    }
}

fn tall_box() -> Box<dyn LayoutItem> {
    Box::new(
        Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap(),
    )
}

fn element_of(node: &RenderNode) -> Arc<Element> {
    match node {
        RenderNode::Element { element, .. } => element.clone(),
        _ => panic!("a document backend gets an element"),
    }
}

#[test]
fn the_page_says_it_is_the_primary_scroll() {
    let _document = AsADocument::new();
    reset_layout_runtime();
    let mut page = ScrollPage::new(tall_box()).unwrap();
    page.relayout(400.0, 300.0);
    assert!(element_of(&page.view()).primary_scroll);
}

#[test]
fn a_platform_reads_the_page_offset_the_surface_reported() {
    let _document = AsADocument::new();
    reset_layout_runtime();
    let mut page = ScrollPage::new(tall_box()).unwrap();
    page.relayout(400.0, 300.0);
    let id = element_of(&page.view()).id.0;
    page.on_event(&Event::BoxScrolled {
        box_id: id,
        x: 0.0,
        y: 240.0,
    });
    assert_eq!(platform_core::primary_scroll_offset(), Some((0.0, 240.0)));
}

#[test]
fn a_platform_moving_the_page_asks_the_surface_in_the_next_frame() {
    let _document = AsADocument::new();
    reset_layout_runtime();
    let mut page = ScrollPage::new(tall_box()).unwrap();
    page.relayout(400.0, 300.0);
    assert!(platform_core::scroll_primary_to(0.0, 500.0));
    assert_eq!(page.viewport().offset().1.get(), 500.0);
    assert_eq!(element_of(&page.view()).scroll_to, Some((0.0, 500.0)));
    assert_eq!(
        element_of(&page.view()).scroll_to,
        None,
        "one frame carries the request"
    );
}

#[test]
fn a_reveal_reaches_a_surface_that_holds_the_content() {
    let _document = AsADocument::new();
    reset_layout_runtime();
    let content = tall_box();
    let content_node = content.layout_node();
    let mut page = ScrollPage::new(content).unwrap();
    page.relayout(400.0, 300.0);
    let id = element_of(&page.view()).id.0;
    page.on_event(&Event::BoxScrolled {
        box_id: id,
        x: 0.0,
        y: 500.0,
    });
    page.viewport().reveal(content_node, 0.0);
    assert_eq!(element_of(&page.view()).scroll_to, Some((0.0, 0.0)));
}

#[test]
fn a_dropped_page_is_no_longer_the_primary_scroll() {
    reset_layout_runtime();
    let page = ScrollPage::new(tall_box()).unwrap();
    assert!(platform_core::primary_scroll_offset().is_some());
    drop(page);
    assert_eq!(platform_core::primary_scroll_offset(), None);
}

#[test]
fn a_page_built_before_the_old_one_drops_keeps_the_claim() {
    reset_layout_runtime();
    let old = ScrollPage::new(tall_box()).unwrap();
    let new = ScrollPage::new(tall_box()).unwrap();
    drop(old);
    new.viewport().scroll_to(0.0, 30.0);
    assert_eq!(platform_core::primary_scroll_offset(), Some((0.0, 30.0)));
}

#[test]
fn a_page_kept_to_the_safe_area_is_padded_by_it_as_it_moves() {
    reset_layout_runtime();
    let mut page = ScrollPage::new(tall_box()).unwrap().keep_to_safe_area();
    page.relayout(400.0, 300.0);
    let rect = crate::context::track_layout(page.viewport().area()).unwrap();
    let area = || rect.get();
    assert_eq!((area().y, area().height), (0.0, 300.0));
    crate::context::set_safe_area_insets(geometry_core::Insets::new(24.0, 0.0, 48.0, 0.0));
    page.relayout(400.0, 300.0);
    assert_eq!((area().y, area().height), (24.0, 228.0));
}

#[test]
fn the_primary_scroll_is_read_from_outside_the_page_and_followed() {
    use std::cell::Cell;
    use std::rc::Rc;

    reset_layout_runtime();
    let reads = Rc::new(Cell::new(0u32));
    let seen = reactive_core::signal(None::<NodeId>);
    let counted = reads.clone();
    let _reader = reactive_core::effect(move || {
        counted.set(counted.get() + 1);
        seen.set(crate::use_primary_scroll().map(|viewport| viewport.area()));
    });
    assert_eq!(seen.peek(), None, "no page holds the primary scroll yet");

    let page = ScrollPage::new(tall_box()).unwrap();
    assert_eq!(seen.peek(), Some(page.viewport().area()));
    assert_eq!(reads.get(), 2, "the reader ran again when the page took it");

    drop(page);
    assert_eq!(seen.peek(), None, "and again when the page let it go");
}

#[test]
fn the_newest_page_keeps_the_primary_scroll_when_an_older_one_drops() {
    reset_layout_runtime();
    let old = ScrollPage::new(tall_box()).unwrap();
    let new = ScrollPage::new(tall_box()).unwrap();
    drop(old);
    assert_eq!(
        crate::use_primary_scroll().map(|viewport| viewport.area()),
        Some(new.viewport().area())
    );
}

#[test]
fn the_primary_scroll_reports_how_far_the_page_has_scrolled() {
    reset_layout_runtime();
    let mut page = ScrollPage::new(tall_box()).unwrap();
    page.relayout(400.0, 300.0);
    page.viewport().scroll_to(0.0, 350.0);
    let primary = crate::use_primary_scroll().expect("the page holds it");
    assert_eq!(primary.progress(crate::Axis::Vertical), 0.5);
}
