use platform_core::{DragActions, DragData, DropAction, DroppedData, MimeType};

use super::*;

const SOURCE: SurfaceId = SurfaceId(1);
const TARGET: SurfaceId = SurfaceId(2);

fn token() -> MimeType {
    MimeType::from_static("application/x-token")
}

fn outgoing() -> OutgoingDrag {
    OutgoingDrag::new(
        DragData::new()
            .with(token(), b"t".as_slice())
            .with_text("words"),
        DragActions::Copy,
    )
}

fn names(events: &[Event]) -> Vec<&'static str> {
    events
        .iter()
        .map(|event| match event {
            Event::PointerMoved { .. } => "moved",
            Event::PointerPressed { .. } => "pressed",
            Event::PointerReleased { .. } => "released",
            Event::CursorLeft => "cursor-left",
            Event::DragOver { .. } => "drag-over",
            Event::DragLeft => "drag-left",
            Event::Dropped { .. } => "dropped",
            Event::DragEnded { .. } => "drag-ended",
            _ => "other",
        })
        .collect()
}

#[test]
fn without_a_drag_the_pointer_reaches_the_surface_it_is_over() {
    let seat = HeadlessSeat::new();
    seat.script([
        PointerStep::Press {
            surface: SOURCE,
            x: 1.0,
            y: 1.0,
        },
        PointerStep::Release,
    ]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["moved", "pressed"]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["released"]);
    assert!(seat.take_events(SOURCE).is_empty());
    assert!(!seat.is_busy());
}

#[test]
fn a_drag_needs_the_button_still_held_on_the_surface_that_starts_it() {
    let seat = HeadlessSeat::new();
    assert!(!seat.start_drag(SOURCE, outgoing()));
    seat.script([PointerStep::Press {
        surface: SOURCE,
        x: 1.0,
        y: 1.0,
    }]);
    seat.take_events(SOURCE);
    assert!(!seat.start_drag(TARGET, outgoing()));
    assert!(seat.start_drag(SOURCE, outgoing()));
    assert!(seat.is_dragging());
}

#[test]
fn a_drop_on_a_surface_that_answered_delivers_the_answered_type_and_tells_the_source() {
    let seat = HeadlessSeat::new();
    seat.script([PointerStep::Press {
        surface: SOURCE,
        x: 5.0,
        y: 5.0,
    }]);
    seat.take_events(SOURCE);
    assert!(seat.start_drag(SOURCE, outgoing()));
    seat.script([
        PointerStep::Move {
            surface: TARGET,
            x: 20.0,
            y: 30.0,
        },
        PointerStep::Release,
    ]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["drag-over"]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["drag-left"]);
    let over = seat.take_events(TARGET);
    assert_eq!(names(&over), ["drag-over"]);
    seat.answer_drag(
        TARGET,
        Some(DropAnswer {
            mime: MimeType::TEXT_PLAIN_UTF8,
            action: DropAction::Copy,
        }),
    );
    seat.take_events(SOURCE);
    let dropped = seat.take_events(TARGET);
    assert_eq!(
        dropped,
        [Event::Dropped {
            x: 20.0,
            y: 30.0,
            data: DroppedData {
                mime: MimeType::TEXT_PLAIN_UTF8,
                bytes: std::sync::Arc::from(b"words".as_slice()),
                action: DropAction::Copy,
            },
        }]
    );
    assert_eq!(
        seat.take_events(SOURCE),
        [Event::DragEnded {
            outcome: DragOutcome::Dropped(DropAction::Copy),
        }]
    );
    assert!(!seat.is_dragging());
}

#[test]
fn a_drop_over_nothing_that_answered_cancels_the_drag() {
    let seat = HeadlessSeat::new();
    seat.script([PointerStep::Press {
        surface: SOURCE,
        x: 5.0,
        y: 5.0,
    }]);
    seat.take_events(SOURCE);
    assert!(seat.start_drag(SOURCE, outgoing()));
    seat.script([PointerStep::Leave, PointerStep::Release]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["drag-over"]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["drag-left"]);
    assert_eq!(
        seat.take_events(SOURCE),
        [Event::DragEnded {
            outcome: DragOutcome::Cancelled,
        }]
    );
}

#[test]
fn a_step_waits_until_every_surface_it_reached_has_handled_it() {
    let seat = HeadlessSeat::new();
    seat.script([
        PointerStep::Move {
            surface: TARGET,
            x: 1.0,
            y: 1.0,
        },
        PointerStep::Move {
            surface: SOURCE,
            x: 1.0,
            y: 1.0,
        },
    ]);
    assert!(seat.take_events(SOURCE).is_empty());
    assert!(seat.take_events(SOURCE).is_empty());
    assert_eq!(names(&seat.take_events(TARGET)), ["moved"]);
    assert_eq!(names(&seat.take_events(TARGET)), ["cursor-left"]);
    assert_eq!(names(&seat.take_events(SOURCE)), ["moved"]);
}
