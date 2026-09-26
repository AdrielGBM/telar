//! The app's address through the real runner: a headless platform's declared history in, the moves the app makes out.

#[path = "test_common.rs"]
mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use common::FillApp;
use platform_headless::HeadlessPlatform;
use telar::{
    App, AppConfig, AppCtx, AppPathsProvider, Color, Component, HistorySink, Location,
    LocationFormat, NoPaths, location_history, push_location, run_with_platform,
};

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

struct Walker {
    fill: FillApp,
    seen_at_build: Rc<RefCell<Vec<Location>>>,
    goes_to: Option<Location>,
    went: Cell<bool>,
}

impl App for Walker {
    fn root(&self) -> Box<dyn Component> {
        *self.seen_at_build.borrow_mut() = location_history();
        self.fill.root()
    }

    fn on_frame(&mut self, _ctx: &mut AppCtx) {
        if let Some(location) = &self.goes_to
            && !self.went.replace(true)
        {
            push_location(location.clone());
        }
    }
}

fn run(platform: HeadlessPlatform, goes_to: Option<Location>) -> Vec<Location> {
    let seen_at_build = Rc::new(RefCell::new(Vec::new()));
    let app = Walker {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        seen_at_build: seen_at_build.clone(),
        goes_to,
        went: Cell::new(false),
    };
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        app,
        "telar-location-test",
    )
    .expect("headless run failed");
    seen_at_build.take()
}

#[test]
fn the_tree_is_built_already_standing_on_the_declared_location() {
    let seen = run(
        HeadlessPlatform::new(8, 8).with_location([at("/"), at("/es")]),
        None,
    );
    assert_eq!(seen, [at("/"), at("/es")]);
}

#[test]
fn a_move_the_app_makes_reaches_the_platform() {
    let sink = HistorySink::default();
    run(
        HeadlessPlatform::new(8, 8)
            .with_location([at("/es")])
            .record_location_into(sink.clone())
            .with_frames(2),
        Some(at("/es/projects")),
    );
    assert_eq!(*sink.lock().unwrap(), [at("/es"), at("/es/projects")]);
}

#[test]
fn the_platform_moving_by_itself_reaches_the_app() {
    run(
        HeadlessPlatform::new(8, 8)
            .with_location([at("/"), at("/a"), at("/b")])
            .with_location_changes([vec![at("/"), at("/a")]]),
        None,
    );
    assert_eq!(location_history(), [at("/"), at("/a")]);
}
