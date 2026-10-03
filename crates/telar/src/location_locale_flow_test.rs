//! The locale as part of the app's address, through the real runner: a headless platform's declared location in, the locale the app shows and the address it writes out.
//!
//! One test, its scenarios each on a thread of its own: every scenario reads and writes the one process-wide preference store, and a thread is a fresh history and reactive runtime.

#[path = "test_common.rs"]
mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use common::FillApp;
use platform_headless::HeadlessPlatform;
use telar::{
    App, AppConfig, AppCtx, AppPathsProvider, Color, Component, Destination, HistorySink, Location,
    LocationFormat, NoPaths, SystemPreferences, address_of, current_locale, follow_location_locale,
    location_history, run_with_platform, set_locale, store_preference, stored_preference,
    user_preferences::LOCALE_KEY,
};

fn at(path: &str) -> Location {
    LocationFormat::root()
        .with_locales(["es", "en"])
        .parse(path)
        .unwrap()
}

#[derive(Default)]
struct Seen {
    history: Vec<Location>,
    locale: Option<String>,
    link: String,
}

struct Reader {
    fill: FillApp,
    seen: Rc<RefCell<Seen>>,
    switches_to: Option<&'static str>,
    switched: Cell<bool>,
}

impl App for Reader {
    fn root(&self) -> Box<dyn Component> {
        *self.seen.borrow_mut() = Seen {
            history: location_history(),
            locale: current_locale(),
            link: address_of(&Destination::Route(Location::from_segments(["projects"]))),
        };
        self.fill.root()
    }

    fn on_frame(&mut self, _ctx: &mut AppCtx) {
        if let Some(locale) = self.switches_to
            && !self.switched.replace(true)
        {
            set_locale(locale);
        }
    }
}

fn run(platform: HeadlessPlatform, switches_to: Option<&'static str>) -> Seen {
    follow_location_locale(["es", "en"], "es");
    let seen = Rc::new(RefCell::new(Seen::default()));
    let app = Reader {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        seen: seen.clone(),
        switches_to,
        switched: Cell::new(false),
    };
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        app,
        "telar-location-locale-test",
    )
    .expect("headless run failed");
    seen.take()
}

fn preferring(locales: &[&str]) -> SystemPreferences {
    SystemPreferences {
        locales: locales.iter().map(|tag| tag.to_string()).collect(),
        ..SystemPreferences::default()
    }
}

fn on_a_thread_of_its_own(scenario: impl FnOnce() + Send + 'static) {
    std::thread::spawn(scenario)
        .join()
        .expect("the scenario panicked");
}

#[test]
fn the_locale_rides_the_address_through_the_runner() {
    on_a_thread_of_its_own(|| {
        let seen = run(
            HeadlessPlatform::new(8, 8)
                .with_location([at("/en/"), at("/en/projects")])
                .with_system_preferences(preferring(&["es"])),
            None,
        );
        assert_eq!(seen.locale.as_deref(), Some("en"), "the address decides");
        assert_eq!(seen.history, [at("/en/"), at("/en/projects")]);
        assert_eq!(seen.link, "/en/projects", "links are written in it");
        assert_eq!(stored_preference(LOCALE_KEY).as_deref(), Some("en"));
    });

    on_a_thread_of_its_own(|| {
        store_preference(LOCALE_KEY, None);
        let sink = HistorySink::default();
        let seen = run(
            HeadlessPlatform::new(8, 8)
                .with_location([at("/")])
                .with_system_preferences(preferring(&["en-GB", "es"]))
                .record_location_into(sink.clone()),
            None,
        );
        assert_eq!(
            seen.locale.as_deref(),
            Some("en"),
            "an address naming none negotiates the system's preferred locales"
        );
        assert_eq!(*sink.lock().unwrap(), [at("/en/")]);
    });

    on_a_thread_of_its_own(|| {
        store_preference(LOCALE_KEY, Some("es"));
        let seen = run(
            HeadlessPlatform::new(8, 8)
                .with_location([at("/projects")])
                .with_system_preferences(preferring(&["en"])),
            None,
        );
        assert_eq!(
            seen.locale.as_deref(),
            Some("es"),
            "the locale the person last chose outranks the system's"
        );
    });

    on_a_thread_of_its_own(|| {
        let sink = HistorySink::default();
        run(
            HeadlessPlatform::new(8, 8)
                .with_location([at("/es/"), at("/es/#contact")])
                .record_location_into(sink.clone())
                .with_frames(2),
            Some("en"),
        );
        assert_eq!(
            *sink.lock().unwrap(),
            [at("/en/"), at("/en/#contact")],
            "switching keeps the page and the anchor"
        );
        assert_eq!(stored_preference(LOCALE_KEY).as_deref(), Some("en"));
    });
}
