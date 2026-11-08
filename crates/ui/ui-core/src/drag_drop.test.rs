use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{DragActions, DragData, PointerSource};
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult};

use super::*;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::LayoutItem;
use crate::styled_container::StyledContainer;

fn token() -> MimeType {
    MimeType::from_static("application/x-token")
}

fn offer(data: DragData) -> DragOffer {
    DragOffer::of(&data, DragActions::CopyOrMove)
}

fn over(x: f64, y: f64, offer: DragOffer) -> Event {
    Event::DragOver { x, y, offer }
}

fn answers() -> Vec<DropAnswer> {
    platform_core::take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            WindowCommand::AcceptDrop(answer) => Some(answer),
            _ => None,
        })
        .collect()
}

type Log = Rc<RefCell<Vec<String>>>;

fn target(name: &'static str, accepts: Vec<MimeType>, log: &Log) -> StyledContainer {
    let hovers = log.clone();
    let drops = log.clone();
    StyledContainer::new(
        LayoutStyle::new().width(100.0).height(50.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .drop_target(
        DropTarget::new(accepts),
        move |at| {
            hovers
                .borrow_mut()
                .push(format!("{name} hover {}", at.is_some()))
        },
        move |data, x, y| {
            drops.borrow_mut().push(format!(
                "{name} got {} {:?} at {x},{y}",
                data.mime,
                String::from_utf8_lossy(&data.bytes)
            ))
        },
    )
}

fn row(children: Vec<Box<dyn LayoutItem>>) -> StyledContainer {
    let row = StyledContainer::new(
        LayoutStyle::new().flex_row().width(200.0).height(50.0),
        |_r| RectStyle::default(),
        children,
    )
    .unwrap();
    compute_layout(
        row.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();
    row
}

#[test]
fn a_target_answers_with_the_first_type_it_prefers_that_the_drag_offers() {
    let target = DropTarget::new([token(), MimeType::TEXT_PLAIN_UTF8]).preferring(DropAction::Move);
    let both = offer(
        DragData::new()
            .with_text("p")
            .with(token(), b"t".as_slice()),
    );
    assert_eq!(
        target.answer(&both),
        Some(DropAnswer {
            mime: token(),
            action: DropAction::Move,
        })
    );
    let copy_only = DragOffer::of(&DragData::new().with_text("p"), DragActions::Copy);
    assert_eq!(
        target.answer(&copy_only),
        Some(DropAnswer {
            mime: MimeType::TEXT_PLAIN_UTF8,
            action: DropAction::Copy,
        })
    );
    assert_eq!(
        target.answer(&offer(
            DragData::new().with(MimeType::URI_LIST, b"x".as_slice())
        )),
        None
    );
}

#[test]
fn the_target_under_the_drag_answers_and_hears_it_come_and_go() {
    reset_layout_runtime();
    platform_core::take_window_commands();
    let log: Log = Rc::default();
    let mut row = row(vec![
        Box::new(target("text", vec![MimeType::TEXT_PLAIN_UTF8], &log)),
        Box::new(target("token", vec![token()], &log)),
    ]);
    let text = offer(DragData::new().with_text("p"));

    assert_eq!(
        row.on_event(&over(20.0, 10.0, text.clone())),
        EventResult::Handled
    );
    assert_eq!(
        answers(),
        [DropAnswer {
            mime: MimeType::TEXT_PLAIN_UTF8,
            action: DropAction::Copy,
        }]
    );
    assert_eq!(
        row.on_event(&over(150.0, 10.0, text.clone())),
        EventResult::Ignored
    );
    assert!(answers().is_empty(), "the token target refuses plain text");
    row.on_event(&over(30.0, 10.0, text));
    row.on_event(&Event::DragLeft);
    assert_eq!(
        *log.borrow(),
        [
            "text hover true",
            "text hover false",
            "text hover true",
            "text hover false"
        ]
    );
}

#[test]
fn a_drop_reaches_the_target_under_it_in_its_own_coordinates() {
    reset_layout_runtime();
    let log: Log = Rc::default();
    let mut row = row(vec![
        Box::new(target("text", vec![MimeType::TEXT_PLAIN_UTF8], &log)),
        Box::new(target("token", vec![token()], &log)),
    ]);
    let dropped = Event::Dropped {
        x: 130.0,
        y: 20.0,
        data: DroppedData {
            mime: token(),
            bytes: Arc::from(b"t-1".as_slice()),
            action: DropAction::Copy,
        },
    };
    assert_eq!(row.on_event(&dropped), EventResult::Handled);
    assert_eq!(
        *log.borrow(),
        ["token got application/x-token \"t-1\" at 30,20"]
    );
}

#[test]
fn an_inner_target_takes_the_drag_from_the_one_around_it() {
    reset_layout_runtime();
    platform_core::take_window_commands();
    let log: Log = Rc::default();
    let inner = target("inner", vec![MimeType::TEXT_PLAIN_UTF8], &log);
    let outer_log = log.clone();
    let mut outer = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(50.0),
        |_r| RectStyle::default(),
        vec![Box::new(inner)],
    )
    .unwrap()
    .drop_target(
        DropTarget::new([MimeType::TEXT_PLAIN_UTF8]),
        move |at| {
            outer_log
                .borrow_mut()
                .push(format!("outer hover {}", at.is_some()))
        },
        |_, _, _| {},
    );
    compute_layout(
        outer.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();
    let text = offer(DragData::new().with_text("p"));
    outer.on_event(&over(20.0, 10.0, text.clone()));
    assert_eq!(answers().len(), 1, "one answer for one hover");
    outer.on_event(&over(150.0, 10.0, text));
    assert_eq!(
        *log.borrow(),
        ["inner hover true", "inner hover false", "outer hover true"]
    );
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

#[test]
fn a_source_starts_the_drag_only_past_a_taps_slop_and_hears_how_it_ended() {
    reset_layout_runtime();
    platform_core::take_window_commands();
    let ended: Rc<RefCell<Vec<DragOutcome>>> = Rc::default();
    let taps = Rc::new(std::cell::Cell::new(0));
    let sink = ended.clone();
    let tapped = taps.clone();
    let mut source = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(50.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || tapped.set(tapped.get() + 1))
    .drag_source(
        || {
            Some(OutgoingDrag::new(
                DragData::new().with_text("p"),
                DragActions::Copy,
            ))
        },
        move |outcome| sink.borrow_mut().push(outcome),
    );
    compute_layout(
        source.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();

    source.on_event(&press(10.0, 10.0));
    source.on_event(&moved(10.0 + f64::from(TAP_SLOP), 10.0));
    assert!(
        platform_core::take_window_commands().is_empty(),
        "within the slop it is still a tap"
    );
    source.on_event(&moved(40.0, 10.0));
    let started: Vec<_> = platform_core::take_window_commands()
        .into_iter()
        .filter(|command| matches!(command, WindowCommand::StartDrag(_)))
        .collect();
    assert_eq!(started.len(), 1);
    source.on_event(&Event::PointerReleased {
        x: 40.0,
        y: 10.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert_eq!(taps.get(), 0, "a stroke that became a drag is no tap");
    source.on_event(&Event::DragEnded {
        outcome: DragOutcome::Dropped(DropAction::Copy),
    });
    source.on_event(&Event::DragEnded {
        outcome: DragOutcome::Cancelled,
    });
    assert_eq!(*ended.borrow(), [DragOutcome::Dropped(DropAction::Copy)]);
}
