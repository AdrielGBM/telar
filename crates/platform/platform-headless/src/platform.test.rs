use std::cell::RefCell;
use std::rc::Rc;

use platform_core::{
    ColorScheme, Event, EventHandler, MultiSurfacePlatform, Platform, SurfaceId, SystemPreferences,
    Window, WindowConfig,
};

use super::HeadlessPlatform;
use crate::HeadlessWindow;

#[derive(Debug, PartialEq)]
enum Step {
    Preferences(SystemPreferences),
    Resumed,
}

struct Recorder(Rc<RefCell<Vec<Step>>>);

impl EventHandler<HeadlessWindow> for Recorder {
    fn on_resume(&mut self, _window: &HeadlessWindow) -> bool {
        self.0.borrow_mut().push(Step::Resumed);
        true
    }

    fn on_event(&mut self, event: Event, _window: &HeadlessWindow) {
        if let Event::SystemPreferencesChanged { preferences } = event {
            self.0.borrow_mut().push(Step::Preferences(preferences));
        }
    }

    fn on_redraw(&mut self, _window: &HeadlessWindow) {}
}

fn declared() -> SystemPreferences {
    SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        reduced_motion: Some(true),
        high_contrast: None,
        locales: vec!["es".into(), "en".into()],
    }
}

#[test]
fn declared_preferences_arrive_before_the_first_resume() {
    let log = Rc::new(RefCell::new(Vec::new()));
    HeadlessPlatform::new(4, 4)
        .with_system_preferences(declared())
        .run(WindowConfig::default(), Recorder(log.clone()))
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [Step::Preferences(declared()), Step::Resumed]
    );
}

#[test]
fn undeclared_preferences_are_unknown_rather_than_defaulted() {
    let log = Rc::new(RefCell::new(Vec::new()));
    HeadlessPlatform::new(4, 4)
        .run(WindowConfig::default(), Recorder(log.clone()))
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [
            Step::Preferences(SystemPreferences::default()),
            Step::Resumed
        ]
    );
}

#[test]
fn every_surface_is_told_before_it_resumes() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let factory_log = log.clone();
    HeadlessPlatform::new(4, 4)
        .with_system_preferences(declared())
        .run_surfaces(
            vec![
                (SurfaceId(1), WindowConfig::default()),
                (SurfaceId(2), WindowConfig::default()),
            ],
            move |_| Recorder(factory_log.clone()),
        )
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [
            Step::Preferences(declared()),
            Step::Resumed,
            Step::Preferences(declared()),
            Step::Resumed,
        ]
    );
}

#[derive(Debug, PartialEq)]
enum Frame {
    Resized {
        event: (u32, u32),
        window: (u32, u32),
    },
    Drawn {
        window: (u32, u32),
    },
}

struct FrameLog(Rc<RefCell<Vec<Frame>>>);

impl EventHandler<HeadlessWindow> for FrameLog {
    fn on_resume(&mut self, _window: &HeadlessWindow) -> bool {
        true
    }

    fn on_event(&mut self, event: Event, window: &HeadlessWindow) {
        if let Event::WindowResized { width, height } = event {
            self.0.borrow_mut().push(Frame::Resized {
                event: (width, height),
                window: (window.width(), window.height()),
            });
        }
    }

    fn on_redraw(&mut self, window: &HeadlessWindow) {
        self.0.borrow_mut().push(Frame::Drawn {
            window: (window.width(), window.height()),
        });
    }
}

#[test]
fn each_scripted_resize_lands_before_its_frame_and_the_window_agrees() {
    let log = Rc::new(RefCell::new(Vec::new()));
    HeadlessPlatform::new(100, 50)
        .with_resizes([(300, 200), (40, 20)])
        .run(WindowConfig::default(), FrameLog(log.clone()))
        .unwrap();
    assert_eq!(
        *log.borrow(),
        [
            Frame::Resized {
                event: (300, 200),
                window: (300, 200)
            },
            Frame::Drawn { window: (300, 200) },
            Frame::Resized {
                event: (40, 20),
                window: (40, 20)
            },
            Frame::Drawn { window: (40, 20) },
        ]
    );
}

fn at(path: &str) -> platform_core::Location {
    platform_core::LocationFormat::root().parse(path).unwrap()
}

#[test]
fn a_declared_location_is_what_the_app_opens_on_and_moves_are_recorded() {
    let sink = platform_core::HistorySink::default();
    let mut platform = HeadlessPlatform::new(4, 4)
        .record_location_into(sink.clone())
        .with_location([at("/"), at("/es")]);
    let mut source = platform
        .location_source()
        .expect("headless always has an address");
    assert_eq!(source.initial(), [at("/"), at("/es")]);
    source.push(&[at("/"), at("/es"), at("/es/a")]);
    assert_eq!(sink.lock().unwrap().len(), 3);
}

#[test]
fn an_undeclared_location_opens_the_app_at_its_root() {
    let mut source = HeadlessPlatform::new(4, 4).location_source().unwrap();
    assert!(source.initial().is_empty());
}

struct HistoryLog(Rc<RefCell<Vec<String>>>);

impl EventHandler<HeadlessWindow> for HistoryLog {
    fn on_resume(&mut self, _window: &HeadlessWindow) -> bool {
        true
    }

    fn on_event(&mut self, event: Event, _window: &HeadlessWindow) {
        if let Event::LocationChanged { history } = event {
            self.0
                .borrow_mut()
                .push(format!("moved to {}", history.len()));
        }
    }

    fn on_redraw(&mut self, _window: &HeadlessWindow) {
        self.0.borrow_mut().push("drawn".into());
    }
}

#[test]
fn each_scripted_location_change_lands_before_its_frame() {
    let log = Rc::new(RefCell::new(Vec::new()));
    HeadlessPlatform::new(4, 4)
        .with_location_changes([vec![at("/")], vec![at("/"), at("/a")]])
        .run(WindowConfig::default(), HistoryLog(log.clone()))
        .unwrap();
    assert_eq!(
        *log.borrow(),
        ["moved to 1", "drawn", "moved to 2", "drawn"]
    );
}
