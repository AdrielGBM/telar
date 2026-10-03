//! A page written ahead of time is the page the runner builds: the same inputs give the same tree, the same layout nodes and so the same element ids, whichever of the two built it.

use std::sync::Arc;

use platform_headless::HeadlessPlatform;
use renderer_record::{Recording, RecordingFactory};
use telar::{
    App, AppConfig, AppPathsProvider, Color, Component, Container, LayoutStyle, LocationFormat,
    NoPaths, Role, SizeDimension, SystemPreferences, Text, TextStyle, WindowRoot, hot_signal,
    location_history, prerender_page, reset_layout_runtime, run_with_platform_and_renderer,
};
use telar_project::{
    PageLocation, PageRequest, Preferences, PrerenderRequest, PrerenderedPage, Surface,
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

struct Site;

impl App for Site {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let page = location_history().last().cloned().unwrap_or_default();
        let visits = hot_signal("site::visits", 3u32);
        let heading = Text::new(
            move || format!("Page /{}", page.segments().join("/")),
            LayoutStyle::new(),
            || TextStyle::new(24.0, Color::BLACK),
        )
        .unwrap();
        let body = Text::new(
            move || format!("Welcome & <hello>, visit {}", visits.get()),
            LayoutStyle::new(),
            || TextStyle::new(16.0, Color::BLACK),
        )
        .unwrap();
        let section = Container::new(LayoutStyle::new().flex_column(), vec![Box::new(body)])
            .unwrap()
            .role(Role::Section);
        let main = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            vec![Box::new(heading), Box::new(section)],
        )
        .unwrap()
        .role(Role::Main);
        Box::new(WindowRoot::new(Box::new(main)))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::WHITE)
    }
}

fn request(page: PageRequest) -> PrerenderRequest {
    PrerenderRequest {
        out: std::env::temp_dir().join("telar-prerender-flow.json"),
        page,
        surface: Surface {
            width: WIDTH,
            height: HEIGHT,
        },
        preferences: Preferences {
            color_scheme: Some("light".to_string()),
            ..Preferences::default()
        },
    }
}

fn docs() -> PageRequest {
    PageRequest::At {
        location: PageLocation {
            segments: vec!["docs".to_string()],
            locale: None,
        },
    }
}

fn written(page: PageRequest) -> PrerenderedPage {
    prerender_page(
        AppConfig::default(),
        Site,
        "telar-prerender-test",
        &request(page),
    )
}

/// The ids a document's elements carry, in document order.
fn ids_in(markup: &str) -> Vec<u64> {
    markup
        .split("data-telar-id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next()?.parse().ok())
        .collect()
}

/// The ids of the boxes a frame opens, in the order it opens them, which is document order.
fn ids_of(commands: &[telar::DrawCommand]) -> Vec<u64> {
    commands
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::PushElement { element } => Some(element.id.0),
            _ => None,
        })
        .collect()
}

#[test]
fn the_runner_builds_the_tree_the_page_was_written_from() {
    let page = written(docs());
    assert!(page.settled);
    let prerendered = ids_in(&page.markup);
    assert!(prerendered.len() >= 4, "{}", page.markup);

    let recording = Recording::new();
    let was = ui_tree::set_element_capture(true);
    let at = LocationFormat::root().parse("/docs").unwrap();
    run_with_platform_and_renderer::<_, _, _, ()>(
        HeadlessPlatform::new(WIDTH, HEIGHT)
            .with_location([at])
            .with_system_preferences(SystemPreferences {
                color_scheme: Some(telar::ColorScheme::Light),
                ..SystemPreferences::default()
            })
            .with_frames(3),
        RecordingFactory::new(recording.clone()),
        AppConfig::default(),
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        Site,
        "telar-prerender-test",
    )
    .expect("headless run failed");
    ui_tree::set_element_capture(was);

    let frame = recording.last_frame().expect("the runner drew a frame");
    assert_eq!(ids_of(&frame.commands), prerendered);
}

#[test]
fn the_same_inputs_write_the_same_page() {
    let first = written(docs());
    let second = written(docs());
    assert_eq!(first, second);
}

#[test]
fn a_page_carries_what_it_was_built_from() {
    let page = written(docs());
    assert_eq!(page.state.location.as_deref(), Some("/docs"));
    assert_eq!(page.state.surface.width, WIDTH);
    assert_eq!(
        page.state.preferences.color_scheme.as_deref(),
        Some("light")
    );
    assert_eq!(
        page.state.signals.get("site::visits").map(String::as_str),
        Some("3")
    );
    assert!(page.markup.contains("Page /docs"), "{}", page.markup);
    assert!(
        page.markup.contains("Welcome &amp; &lt;hello&gt;"),
        "{}",
        page.markup
    );
    assert!(
        page.markup.starts_with("<main data-telar-id="),
        "{}",
        page.markup
    );
    assert!(page.head.starts_with("<style id=\"telar-reset\">"));
    assert!(
        page.host_attributes
            .iter()
            .any(|(name, value)| name == "style" && value.contains("background-color:#ffffff")),
        "{:?}",
        page.host_attributes
    );
}

#[test]
fn the_page_for_an_address_with_none_names_no_location() {
    let page = written(PageRequest::NotFound);
    assert_eq!(page.state.location, None);
    assert!(!page.markup.is_empty());
}
