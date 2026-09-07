use std::cell::RefCell;
use std::rc::Rc;

use platform_core::PointerSource;

use super::*;

const RECT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 100.0,
};

fn press_at(x: f32, y: f32) -> Event {
    Event::PointerPressed {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn move_to(x: f32, y: f32) -> Event {
    Event::PointerMoved {
        x: x as f64,
        y: y as f64,
        source: PointerSource::Mouse,
    }
}

/// Records every position the gesture reported, and what it said armed the stroke at the time.
type Log = Rc<RefCell<Vec<((f32, f32), Option<DragStart>)>>>;

fn logging(threshold: f32) -> (DragGesture, Log) {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let mut drag = DragGesture::default();
    drag.set_threshold(threshold);
    let sink = log.clone();
    drag.set(move |x, y| sink.borrow_mut().push(((x, y), drag_start())));
    (drag, log)
}

/// The default, which a slider depends on: pressing the track *is* setting the value, so the press itself reports and waiting for movement would make the first click do nothing.
#[test]
fn without_a_threshold_the_press_itself_reports() {
    let (mut drag, log) = logging(0.0);
    assert_eq!(
        drag.press(&press_at(30.0, 40.0), RECT),
        EventResult::Handled
    );
    assert_eq!(log.borrow().len(), 1, "the press reported straight away");
    assert_eq!(log.borrow()[0].0, (30.0, 40.0));
}

/// And the reading a viewport needs: a stroke that never travelled was a click on whatever sits under it, not a drag of nothing. `end` answering `false` is what leaves the release to the tap gesture.
#[test]
fn a_press_that_never_travels_is_not_a_drag() {
    let (mut drag, log) = logging(4.0);
    drag.press(&press_at(30.0, 40.0), RECT);
    drag.moved(&move_to(32.0, 41.0), RECT);
    assert!(log.borrow().is_empty(), "two pixels is not a drag");
    assert!(!drag.end(None), "so nothing was dragged to end");
}

/// Crossing the threshold starts the drag *where it crossed*. Reporting the press point retroactively would jump whatever is being dragged by the slop distance the instant it started moving.
#[test]
fn crossing_the_threshold_starts_the_drag_where_it_crossed() {
    let (mut drag, log) = logging(4.0);
    drag.press(&press_at(30.0, 40.0), RECT);
    drag.moved(&move_to(32.0, 40.0), RECT);
    drag.moved(&move_to(50.0, 40.0), RECT);

    assert_eq!(log.borrow().len(), 1, "only the move that cleared it");
    assert_eq!(log.borrow()[0].0, (50.0, 40.0), "and not back at the press");
    assert!(drag.end(None), "this one really was a drag");
}

/// Mode dispatch, and the reason it is frozen: a hand that lets go of Shift halfway through would turn an orbit into a pan mid-stroke if the gesture asked what is held *now*.
#[test]
fn a_drag_reports_what_armed_it_and_not_what_is_held_now() {
    crate::keyboard::reset();
    crate::keyboard::observe(&Event::ModifiersChanged {
        modifiers: ModifiersState {
            is_shift: true,
            ..Default::default()
        },
    });
    let (mut drag, log) = logging(0.0);
    drag.press(&press_at(10.0, 10.0), RECT);

    crate::keyboard::observe(&Event::ModifiersChanged {
        modifiers: ModifiersState::default(),
    });
    drag.moved(&move_to(40.0, 10.0), RECT);
    crate::keyboard::reset();

    let entries = log.borrow();
    assert!(
        entries.iter().all(|(_, start)| start
            .is_some_and(|s| s.modifiers.is_shift && s.button == PointerButton::Primary)),
        "every report names the press, including the one after Shift was released: {entries:?}"
    );
    assert_eq!(
        drag_start(),
        None,
        "and nothing leaks out of the callback it was scoped to"
    );
}
