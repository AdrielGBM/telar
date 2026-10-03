//! The surface's title through the real runner: what the window opens with, and what it is called on each page and in each locale.

#[path = "test_common.rs"]
mod common;

use std::cell::Cell;
use std::sync::Arc;

use common::FillApp;
use platform_headless::{HeadlessPlatform, TitleSink};
use telar::i18n::{Catalog, Entry, Message};
use telar::{
    App, AppConfig, AppCtx, AppPathsProvider, Color, Component, Location, LocationFormat, NoPaths,
    WindowConfig, effect, location_history, run_with_platform, set_catalog, set_locale,
    set_page_title,
};

static CATALOG: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "es",
    entries: &[
        Entry {
            key: "credits.title",
            messages: &[
                ("en", Message::Plain("Credits")),
                ("es", Message::Plain("Créditos")),
            ],
        },
        Entry {
            key: "home.title",
            messages: &[
                ("en", Message::Plain("Home")),
                ("es", Message::Plain("Inicio")),
            ],
        },
    ],
};

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

/// Names each page from the catalog, the way a route's title does, and switches language on the frame it is told to.
struct Titled {
    fill: FillApp,
    switch_to: Option<&'static str>,
    switched: Cell<bool>,
}

impl App for Titled {
    fn root(&self) -> Box<dyn Component> {
        let page = location_history().last().cloned().unwrap_or_default();
        let key = match page.segments() {
            [credits] if credits == "credits" => "credits.title",
            _ => "home.title",
        };
        effect(move || set_page_title(Some(telar::i18n::t(key, &[]))));
        self.fill.root()
    }

    fn on_frame(&mut self, _ctx: &mut AppCtx) {
        if let Some(locale) = self.switch_to
            && !self.switched.replace(true)
        {
            set_locale(locale);
        }
    }
}

fn titles_of(location: &str, locale: &str, switch_to: Option<&'static str>) -> Vec<String> {
    set_catalog(&CATALOG);
    set_locale(locale);
    let sink = TitleSink::default();
    let app = Titled {
        fill: FillApp {
            color: Color::from_rgb_u8(1, 2, 3),
        },
        switch_to,
        switched: Cell::new(false),
    };
    let config = AppConfig::from(WindowConfig {
        title: "Portfolio".to_string(),
        ..WindowConfig::default()
    });
    run_with_platform::<_, _, ()>(
        HeadlessPlatform::new(8, 8)
            .with_location([at(location)])
            .record_titles_into(sink.clone())
            .with_frames(3),
        config,
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        app,
        "telar-title-test",
    )
    .expect("headless run failed");
    sink.lock().unwrap().clone()
}

#[test]
fn the_window_opens_on_the_app_title_and_then_names_the_page() {
    assert_eq!(
        titles_of("/credits", "es", None),
        ["Portfolio", "Créditos — Portfolio"]
    );
}

#[test]
fn every_location_and_locale_has_its_own_title() {
    let last = |location, locale| titles_of(location, locale, None).pop().unwrap();
    assert_eq!(last("/", "es"), "Inicio — Portfolio");
    assert_eq!(last("/", "en"), "Home — Portfolio");
    assert_eq!(last("/credits", "en"), "Credits — Portfolio");
}

#[test]
fn a_change_of_locale_renames_the_window() {
    assert_eq!(
        titles_of("/credits", "es", Some("en")),
        ["Portfolio", "Créditos — Portfolio", "Credits — Portfolio"]
    );
}
