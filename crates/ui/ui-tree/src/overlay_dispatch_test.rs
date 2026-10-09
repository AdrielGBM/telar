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

/// A bar fixed over the page: it takes the pointer over its own boxes, which sit inside a surface-sized rect it does not otherwise claim.
struct LayerSink {
    boxes: Vec<Rect>,
    hits: Rc<Cell<u32>>,
}

impl OverlaySink for LayerSink {
    fn content_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, 800.0, 600.0)
    }
    fn dispatch(&self, _event: &Event) -> EventResult {
        self.hits.set(self.hits.get() + 1);
        EventResult::Ignored
    }
    fn hits(&self, x: f32, y: f32) -> bool {
        self.boxes.iter().any(|rect| rect.contains(x, y))
    }
    fn fixed(&self) -> bool {
        true
    }
}

fn layer(boxes: Vec<Rect>) -> (Rc<dyn OverlaySink>, Rc<Cell<u32>>) {
    let hits = Rc::new(Cell::new(0));
    let sink: Rc<dyn OverlaySink> = Rc::new(LayerSink {
        boxes,
        hits: Rc::clone(&hits),
    });
    (sink, hits)
}

#[test]
fn a_fixed_layer_takes_the_pointer_only_over_its_own_boxes() {
    reset();
    let (bar, bar_hits) = layer(vec![Rect::new(0.0, 0.0, 800.0, 48.0)]);
    let id = register_overlay(bar);

    assert_eq!(
        dispatch_overlays(&press(400.0, 20.0)),
        EventResult::Handled,
        "a press on the bar's own box is its, even where no child handled it"
    );
    dispatch_overlays(&released(400.0, 20.0));
    assert_eq!(bar_hits.get(), 2);
    assert_eq!(
        dispatch_overlays(&press(400.0, 300.0)),
        EventResult::Ignored,
        "below the bar the press falls through to the page"
    );
    assert_eq!(bar_hits.get(), 2);

    unregister_overlay(id);
}

#[test]
fn an_overlay_registered_before_a_fixed_layer_is_still_hit_first() {
    reset();
    let (dialog, dialog_hits) = sink(Rect::new(0.0, 0.0, 800.0, 600.0));
    let (bar, bar_hits) = layer(vec![Rect::new(0.0, 0.0, 800.0, 48.0)]);
    let d = register_overlay(dialog);
    let b = register_overlay(bar);

    dispatch_overlays(&press(400.0, 20.0));
    assert_eq!(dialog_hits.get(), 1, "the dialog is drawn over the bar");
    assert_eq!(
        bar_hits.get(),
        0,
        "so the bar under it does not get the press"
    );

    unregister_overlay(b);
    unregister_overlay(d);
}

/// A bar fixed over the page that writes down what reached it, to tell a move from a leave.
struct LoggingLayer {
    bar: Rect,
    log: Rc<std::cell::RefCell<Vec<&'static str>>>,
}

impl OverlaySink for LoggingLayer {
    fn content_rect(&self) -> Rect {
        self.bar
    }
    fn dispatch(&self, event: &Event) -> EventResult {
        self.log.borrow_mut().push(match event {
            Event::PointerMoved { .. } => "moved",
            Event::CursorLeft => "left",
            _ => "other",
        });
        EventResult::Ignored
    }
    fn hits(&self, x: f32, y: f32) -> bool {
        self.bar.contains(x, y)
    }
    fn fixed(&self) -> bool {
        true
    }
}

fn logging_bar() -> (
    Rc<dyn OverlaySink>,
    Rc<std::cell::RefCell<Vec<&'static str>>>,
) {
    let log = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink: Rc<dyn OverlaySink> = Rc::new(LoggingLayer {
        bar: Rect::new(0.0, 0.0, 800.0, 48.0),
        log: Rc::clone(&log),
    });
    (sink, log)
}

#[test]
fn a_move_off_a_fixed_layer_tells_it_the_pointer_left() {
    reset();
    let (bar, log) = logging_bar();
    let id = register_overlay(bar);

    dispatch_overlays(&moved(400.0, 20.0));
    assert_eq!(
        dispatch_overlays(&moved(400.0, 300.0)),
        EventResult::Ignored,
        "the move itself is the page's"
    );
    dispatch_overlays(&moved(400.0, 320.0));
    assert_eq!(
        *log.borrow(),
        ["moved", "left"],
        "told once, on the move that left it"
    );

    unregister_overlay(id);
}

#[test]
fn a_move_taken_by_an_overlay_above_leaves_the_layer_under_it() {
    reset();
    let (bar, log) = logging_bar();
    let b = register_overlay(bar);
    dispatch_overlays(&moved(400.0, 20.0));

    let (dialog, _hits) = sink(Rect::new(0.0, 0.0, 800.0, 600.0));
    let d = register_overlay(dialog);
    assert_eq!(dispatch_overlays(&moved(400.0, 21.0)), EventResult::Handled);
    assert_eq!(*log.borrow(), ["moved", "left"]);

    unregister_overlay(d);
    unregister_overlay(b);
}

#[test]
fn a_captured_gesture_leaves_every_other_overlay() {
    reset();
    let (bar, log) = logging_bar();
    let b = register_overlay(bar);
    let (panel, _hits) = sink(Rect::new(0.0, 100.0, 200.0, 200.0));
    let p = register_overlay(panel);

    dispatch_overlays(&moved(400.0, 20.0));
    dispatch_overlays(&press(50.0, 150.0));
    dispatch_overlays(&moved(400.0, 20.0));
    assert_eq!(
        *log.borrow(),
        ["moved", "left"],
        "the drag belongs to the panel wherever it goes, so the bar under it is not hovered"
    );

    unregister_overlay(p);
    unregister_overlay(b);
}

#[test]
fn the_pointer_leaving_the_window_is_the_trees_to_deliver() {
    reset();
    let (bar, log) = logging_bar();
    let id = register_overlay(bar);
    dispatch_overlays(&moved(400.0, 20.0));

    assert_eq!(dispatch_overlays(&Event::CursorLeft), EventResult::Ignored);
    dispatch_overlays(&moved(400.0, 300.0));
    assert_eq!(
        *log.borrow(),
        ["moved"],
        "the tree walk already carried the leave to it, so the next move has nobody left to tell"
    );

    unregister_overlay(id);
}

#[test]
fn an_overlay_taken_down_is_not_told_the_pointer_left() {
    reset();
    let (bar, log) = logging_bar();
    let id = register_overlay(bar);
    dispatch_overlays(&moved(400.0, 20.0));
    unregister_overlay(id);

    dispatch_overlays(&moved(400.0, 300.0));
    assert_eq!(*log.borrow(), ["moved"]);
}
