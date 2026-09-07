use super::scrollbar_tests::{moved, press};
use super::tests::make_scroll_area;
use super::*;

/// Turns element capture on for one test and puts it back, so a test that panics does not leave every later one running as a document backend.
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

/// The whole of the bug: the drag was refused outright where the surface owns the offset, because moving the signal alone would be overruled by the next offset the surface reported. It is a request now.
#[test]
fn the_thumb_can_be_dragged_and_asks_the_surface_to_move() {
    let _document = AsADocument::new();
    let mut sa = make_scroll_area();
    assert_eq!(
        sa.on_event(&press(396.0, 45.0)),
        EventResult::Handled,
        "the press lands on the thumb rather than falling through to the content"
    );
    sa.on_event(&moved(396.0, 150.0));
    assert_eq!(
        sa.core.scroll_y.get(),
        350.0,
        "half the travel, half the range"
    );
    assert_eq!(
        sa.core.commanded.get(),
        Some((0.0, 350.0)),
        "and the surface is asked to put the content there"
    );
}

/// One frame, one request. A box that cannot go as far as it was asked reports the offset it settled on, and a request left standing would drag it back there on every later frame.
#[test]
fn a_request_is_taken_by_the_frame_that_carries_it() {
    use crate::canvas::Canvas;
    use crate::context::{compute_layout, new_container, reset_layout_runtime};
    use layout_core::AvailableSpace;

    let _document = AsADocument::new();
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let sa = LayoutScrollArea::new(
        LayoutStyle::new().width(400.0).height(300.0),
        Box::new(content),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(300.0),
        &[sa.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    sa.core.command(0.0, 350.0);
    let node = sa.view();
    let asked = match &node {
        RenderNode::Element { element, .. } => element.scroll_to,
        _ => panic!("a document backend gets an element"),
    };
    assert_eq!(asked, Some((0.0, 350.0)), "the frame carries the request");
    assert_eq!(
        sa.core.commanded.get(),
        None,
        "and takes it, so no later frame repeats it"
    );
}

/// Where the content already is beats where this widget was about to ask for it: a fling is an offset reported a frame late, and answering it with a stale request stops it dead.
#[test]
fn a_scroll_the_surface_reports_drops_the_request_still_pending() {
    let _document = AsADocument::new();
    let mut sa = make_scroll_area();
    sa.on_event(&press(396.0, 45.0));
    sa.on_event(&moved(396.0, 150.0));
    sa.core.follow(0.0, 120.0);
    assert_eq!(sa.core.commanded.get(), None);
    assert_eq!(sa.core.scroll_y.get(), 120.0);
}

/// A target that draws the content at the offset needs no request at all — there the signal *is* where the content is, and a backend asked to scroll a box it does not have would be answering nothing.
#[test]
fn a_target_that_draws_the_offset_asks_for_nothing() {
    let mut sa = make_scroll_area();
    sa.on_event(&press(396.0, 45.0));
    sa.on_event(&moved(396.0, 150.0));
    assert_eq!(sa.core.scroll_y.get(), 350.0);
    assert_eq!(sa.core.commanded.get(), None);
}
