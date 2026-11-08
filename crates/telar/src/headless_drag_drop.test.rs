//! A drag from one headless surface to another, driven through the simulated seat the way a person drives one on a compositor: across two windows of one run, and across two runs on two threads standing in for two processes.

#[path = "test_common.rs"]
mod common;

use std::sync::{Arc, Mutex};

use platform_headless::{HeadlessPlatform, HeadlessSeat, PointerStep};
use telar::{
    App, AppConfig, AppPathsProvider, Component, DragActions, DragData, DragOutcome, DropAction,
    DropTarget, LayoutStyle, MimeType, NoPaths, OutgoingDrag, RectStyle, SizeDimension,
    StyledContainer, SurfaceId, WindowConfig, WindowRoot, reset_layout_runtime,
    run_multi_with_platform, run_with_platform,
};

const SOURCE: SurfaceId = SurfaceId(1);
const TARGET: SurfaceId = SurfaceId(2);

type Log = Arc<Mutex<Vec<String>>>;

fn token() -> MimeType {
    MimeType::from_static("application/x-telar-test-token")
}

#[derive(Clone, Copy)]
enum Role {
    Source,
    Target,
}

struct DragApp {
    role: Role,
    log: Log,
}

fn whole_window() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

impl App for DragApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let pane = StyledContainer::new(whole_window(), |_r| RectStyle::default(), vec![]).unwrap();
        let log = self.log.clone();
        let pane = match self.role {
            Role::Source => pane.drag_source(
                || {
                    Some(OutgoingDrag::new(
                        DragData::new()
                            .with(token(), b"note-7".as_slice())
                            .with_text("the paragraph"),
                        DragActions::Copy,
                    ))
                },
                move |outcome| {
                    log.lock()
                        .unwrap()
                        .push(format!("source ended {outcome:?}"))
                },
            ),
            Role::Target => {
                let hovers = log.clone();
                pane.drop_target(
                    DropTarget::new([token(), MimeType::TEXT_PLAIN_UTF8]),
                    move |at| {
                        if at.is_none() {
                            hovers.lock().unwrap().push("target hover ended".to_owned());
                        }
                    },
                    move |data, _, _| {
                        log.lock().unwrap().push(format!(
                            "target got {} {}",
                            data.mime,
                            String::from_utf8_lossy(&data.bytes)
                        ))
                    },
                )
            }
        };
        Box::new(WindowRoot::new(Box::new(pane)))
    }
}

fn window() -> AppConfig {
    AppConfig::from(WindowConfig {
        width: 200,
        height: 100,
        ..WindowConfig::default()
    })
}

fn press_and_drag_out_of(seat: &HeadlessSeat) {
    seat.script([
        PointerStep::Press {
            surface: SOURCE,
            x: 20.0,
            y: 20.0,
        },
        PointerStep::Move {
            surface: SOURCE,
            x: 80.0,
            y: 20.0,
        },
    ]);
}

// Sorted, because which of the two windows hears the end of the drop first is the order the run happens to poll them in.
fn sorted(log: &Log) -> Vec<String> {
    let mut entries = log.lock().unwrap().clone();
    entries.sort();
    entries
}

fn delivered_to_target() -> Vec<String> {
    let mut entries = vec![
        "target hover ended".to_owned(),
        "target got application/x-telar-test-token note-7".to_owned(),
        format!("source ended {:?}", DragOutcome::Dropped(DropAction::Copy)),
    ];
    entries.sort();
    entries
}

fn run_two_windows(seat: HeadlessSeat, log: Log) {
    run_multi_with_platform(
        HeadlessPlatform::new(1, 1).with_seat(seat),
        vec![(SOURCE, window()), (TARGET, window())],
        |_| Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        move |id| DragApp {
            role: if id == SOURCE {
                Role::Source
            } else {
                Role::Target
            },
            log: log.clone(),
        },
        "telar-headless-drag",
    )
    .expect("the two-window run settles");
}

#[test]
fn a_drop_on_another_window_delivers_the_type_it_prefers_and_tells_the_source() {
    let seat = HeadlessSeat::new();
    let log: Log = Arc::default();
    press_and_drag_out_of(&seat);
    seat.script([
        PointerStep::Move {
            surface: TARGET,
            x: 100.0,
            y: 50.0,
        },
        PointerStep::Release,
    ]);
    run_two_windows(seat, log.clone());
    assert_eq!(sorted(&log), delivered_to_target());
}

#[test]
fn a_drop_outside_every_window_cancels_the_drag() {
    let seat = HeadlessSeat::new();
    let log: Log = Arc::default();
    press_and_drag_out_of(&seat);
    seat.script([PointerStep::Leave, PointerStep::Release]);
    run_two_windows(seat, log.clone());
    assert_eq!(
        sorted(&log),
        [format!("source ended {:?}", DragOutcome::Cancelled)]
    );
}

#[test]
fn two_apps_on_two_threads_exchange_a_drag_through_one_seat() {
    let seat = HeadlessSeat::new();
    let log: Log = Arc::default();
    press_and_drag_out_of(&seat);
    seat.script([
        PointerStep::Move {
            surface: TARGET,
            x: 100.0,
            y: 50.0,
        },
        PointerStep::Release,
    ]);
    let run = |role: Role, surface: SurfaceId| {
        let seat = seat.clone();
        let log = log.clone();
        std::thread::spawn(move || {
            run_with_platform::<_, _, ()>(
                HeadlessPlatform::new(200, 100)
                    .with_seat(seat)
                    .with_surface_id(surface),
                window(),
                Arc::new(NoPaths),
                DragApp { role, log },
                "telar-headless-drag",
            )
        })
    };
    let source = run(Role::Source, SOURCE);
    let target = run(Role::Target, TARGET);
    source.join().unwrap().expect("the source run settles");
    target.join().unwrap().expect("the target run settles");
    assert_eq!(sorted(&log), delivered_to_target());
}
