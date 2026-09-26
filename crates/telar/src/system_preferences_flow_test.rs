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
    mode_in_frame: Rc<RefCell<Option<String>>>,
    snapshots: Rc<RefCell<Vec<SystemPreferences>>>,
    seen_at_build: Rc<RefCell<Option<SystemPreferences>>>,
}

impl App for Recorder {
    fn root(&self) -> Box<dyn Component> {
        *self.seen_at_build.borrow_mut() = Some(system_preferences());
        self.fill.root()
    }

    fn on_frame(&mut self, _ctx: &mut telar::AppCtx) {
        *self.mode_in_frame.borrow_mut() = telar::active_mode();
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
        mode_in_frame: Rc::default(),
        snapshots: Rc::default(),
        seen_at_build: Rc::default(),
    };
    let observed = Recorder {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        mode_in_frame: recorder.mode_in_frame.clone(),
        snapshots: recorder.snapshots.clone(),
        seen_at_build: recorder.seen_at_build.clone(),
    };
    telar::register_mode("day", || {});
    telar::register_mode("night", || {});
    telar::follow_system("day", "night");
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
    assert_eq!(
        observed.mode_in_frame.borrow().as_deref(),
        Some("night"),
        "the theme followed the scheme by the first frame"
    );
}

#[test]
fn an_unknown_scheme_leaves_the_theme_on_its_default() {
    let observed = run(SystemPreferences {
        locales: vec!["fr".into()],
        ..SystemPreferences::default()
    });
    assert_eq!(observed.mode_in_frame.borrow().as_deref(), Some("day"));
    assert_eq!(observed.snapshots.borrow().len(), 1);
}
