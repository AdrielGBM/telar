use std::cell::Cell;

use platform_core::{PointerButton, PointerSource};

use super::*;

struct RecordingSink {
    rect: Rect,
    hits: Rc<Cell<u32>>,
    blocking: bool,
    // `true` mimics a child consuming the event, `false` a click that missed.
    child_handles: bool,
}

impl OverlaySink for RecordingSink {
    fn content_rect(&self) -> Rect {
        self.rect
    }
    fn dispatch(&self, _event: &Event) -> EventResult {
        self.hits.set(self.hits.get() + 1);
        if self.child_handles {
            EventResult::Handled
        } else {
            EventResult::Ignored
        }
    }
    fn blocking(&self) -> bool {
        self.blocking
    }
}

// A blocking overlay whose children always handle: the default for the routing and capture tests.
fn sink(rect: Rect) -> (Rc<dyn OverlaySink>, Rc<Cell<u32>>) {
    configured_sink(rect, true, true)
}

fn configured_sink(
    rect: Rect,
    blocking: bool,
    child_handles: bool,
) -> (Rc<dyn OverlaySink>, Rc<Cell<u32>>) {
    let hits = Rc::new(Cell::new(0));
    let sink: Rc<dyn OverlaySink> = Rc::new(RecordingSink {
        rect,
        hits: Rc::clone(&hits),
        blocking,
        child_handles,
    });
    (sink, hits)
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}
fn moved(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}
fn released(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

#[test]
fn no_overlays_falls_through() {
    reset();
    assert_eq!(dispatch_overlays(&press(10.0, 10.0)), EventResult::Ignored);
}

#[test]
fn press_inside_is_consumed_outside_falls_through() {
    reset();
    let (s, hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
    let id = register_overlay(s);

    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
    assert_eq!(hits.get(), 1);
    dispatch_overlays(&released(50.0, 50.0));
    assert_eq!(
        dispatch_overlays(&press(500.0, 500.0)),
        EventResult::Ignored
    );

    unregister_overlay(id);
}

#[test]
fn topmost_overlay_wins() {
    reset();
    let (bottom, bottom_hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
    let (top, top_hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
    let b = register_overlay(bottom);
    let t = register_overlay(top);

    dispatch_overlays(&press(50.0, 50.0));
    assert_eq!(
        top_hits.get(),
        1,
        "topmost (last registered) receives the press"
    );
    assert_eq!(
        bottom_hits.get(),
        0,
        "the overlay below must not also get it"
    );

    unregister_overlay(t);
    unregister_overlay(b);
}

#[test]
fn capture_routes_moves_and_release_even_outside() {
    reset();
    let (s, hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
    let id = register_overlay(s);

    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
    assert_eq!(
        dispatch_overlays(&moved(500.0, 500.0)),
        EventResult::Handled
    );
    assert_eq!(
        dispatch_overlays(&released(500.0, 500.0)),
        EventResult::Handled
    );
    assert_eq!(hits.get(), 3);
    assert_eq!(
        dispatch_overlays(&press(500.0, 500.0)),
        EventResult::Ignored
    );

    unregister_overlay(id);
}

#[test]
fn unregister_stops_routing_and_clears_capture() {
    reset();
    let (s, _hits) = sink(Rect::new(0.0, 0.0, 100.0, 100.0));
    let id = register_overlay(s);
    dispatch_overlays(&press(50.0, 50.0));
    unregister_overlay(id);
    assert_eq!(
        dispatch_overlays(&released(50.0, 50.0)),
        EventResult::Ignored
    );
    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Ignored);
}

// A modal swallows a press inside its content rect even where no child sits, so a bare scrim reads as modal and nothing behind it receives the press.
#[test]
fn blocking_overlay_consumes_press_in_empty_region() {
    reset();
    let (s, hits) = configured_sink(Rect::new(0.0, 0.0, 100.0, 100.0), true, false);
    let id = register_overlay(s);

    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
    assert_eq!(
        hits.get(),
        1,
        "the modal is still asked to dispatch the press"
    );

    unregister_overlay(id);
}

// A click-through overlay does not consume a press its children ignore: the click landed on the transparent area, not the visible panel, so it falls through to the tree behind.
#[test]
fn click_through_overlay_falls_through_when_child_ignores() {
    reset();
    let (s, hits) = configured_sink(Rect::new(0.0, 0.0, 100.0, 100.0), false, false);
    let id = register_overlay(s);

    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Ignored);
    assert_eq!(
        hits.get(),
        1,
        "the overlay is offered the press before falling through"
    );
    assert_eq!(dispatch_overlays(&moved(50.0, 50.0)), EventResult::Ignored);

    unregister_overlay(id);
}

// It does consume a press when a child handles it, capturing the gesture so the following release routes back to it.
#[test]
fn click_through_overlay_consumes_when_child_handles() {
    reset();
    let (s, hits) = configured_sink(Rect::new(0.0, 0.0, 100.0, 100.0), false, true);
    let id = register_overlay(s);

    assert_eq!(dispatch_overlays(&press(50.0, 50.0)), EventResult::Handled);
    assert_eq!(
        dispatch_overlays(&released(500.0, 500.0)),
        EventResult::Handled
    );
    assert_eq!(hits.get(), 2);

    unregister_overlay(id);
}
