//! The platform's snapshot reaching an application through the real runner: headless platform in, facade store and application hooks out.

#[path = "test_common.rs"]
mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use common::FillApp;
use platform_headless::HeadlessPlatform;
use telar::{
    App, AppConfig, AppPathsProvider, Color, ColorScheme, Component, NoPaths, SystemPreferences,
    run_with_platform, system_preferences,
};

struct Recorder {
    fill: FillApp,
    schemes: Rc<RefCell<Vec<bool>>>,
    snapshots: Rc<RefCell<Vec<SystemPreferences>>>,
    seen_at_build: Rc<RefCell<Option<SystemPreferences>>>,
}

impl App for Recorder {
    fn root(&self) -> Box<dyn Component> {
        *self.seen_at_build.borrow_mut() = Some(system_preferences());
        self.fill.root()
    }

    fn on_color_scheme(&self, dark: bool) {
        self.schemes.borrow_mut().push(dark);
    }

    fn on_system_preferences(&self, preferences: &SystemPreferences) {
        self.snapshots.borrow_mut().push(preferences.clone());
    }
}

fn run(preferences: SystemPreferences) -> Recorder {
    let recorder = Recorder {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        schemes: Rc::default(),
        snapshots: Rc::default(),
        seen_at_build: Rc::default(),
    };
    let observed = Recorder {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        schemes: recorder.schemes.clone(),
        snapshots: recorder.snapshots.clone(),
        seen_at_build: recorder.seen_at_build.clone(),
    };
    run_with_platform::<_, _, ()>(
        HeadlessPlatform::new(8, 8).with_system_preferences(preferences),
        AppConfig::default(),
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        recorder,
        "telar-system-preferences-test",
    )
    .expect("headless run failed");
    observed
}

#[test]
fn the_tree_is_built_already_knowing_the_preferences() {
    let declared = SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        reduced_motion: Some(true),
        high_contrast: Some(false),
        locales: vec!["es-CL".into(), "en".into()],
    };
    let observed = run(declared.clone());
    assert_eq!(observed.seen_at_build.borrow().as_ref(), Some(&declared));
    assert_eq!(*observed.snapshots.borrow(), [declared]);
    assert_eq!(*observed.schemes.borrow(), [true]);
}

#[test]
fn an_unknown_scheme_is_not_reported_as_light() {
    let observed = run(SystemPreferences {
        locales: vec!["fr".into()],
        ..SystemPreferences::default()
    });
    assert!(observed.schemes.borrow().is_empty());
    assert_eq!(observed.snapshots.borrow().len(), 1);
}
