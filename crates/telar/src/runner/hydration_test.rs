use std::collections::BTreeMap;

use platform_core::ColorScheme;
use telar_project::{Preferences, PrerenderState, STATE_ELEMENT_ID, STATE_VERSION, Surface};

use super::Hydration;

/// The state the browser half is tested reading (`platform-web`'s `served_test.rs`).
const READ_BY_THE_BROWSER: &str =
    include_str!("../../../platform/platform-web/src/served_state_fixture.json");

#[test]
fn the_client_reads_the_format_the_packager_writes() {
    assert_eq!(super::STATE_VERSION, STATE_VERSION);
    assert_eq!(super::STATE_ELEMENT_ID, STATE_ELEMENT_ID);
    let written = PrerenderState {
        version: STATE_VERSION,
        location: Some("/es/proyectos".to_string()),
        locale: Some("es".to_string()),
        preferences: Preferences {
            color_scheme: Some("dark".to_string()),
            reduced_motion: Some(true),
            high_contrast: None,
            locales: vec!["es-CL".to_string(), "en".to_string()],
        },
        surface: Surface {
            width: 1280,
            height: 800,
        },
        signals: BTreeMap::from([
            ("site::visits".to_string(), "3".to_string()),
            ("@telar/theme.scheme".to_string(), "system".to_string()),
        ]),
    };
    let read: serde_json::Value = serde_json::from_str(READ_BY_THE_BROWSER).unwrap();
    assert_eq!(serde_json::to_value(&written).unwrap(), read);
}

#[test]
fn a_state_in_another_format_builds_from_nothing() {
    let state = |version| {
        Hydration::new(
            version,
            platform_core::SystemPreferences::default(),
            (1280, 800),
            None,
            Vec::new(),
        )
    };
    assert!(state(STATE_VERSION).is_some());
    assert!(state(STATE_VERSION + 1).is_none());
}

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

/// An app whose tree is shaped by the scheme it is first built under, with a keyed signal and an entrance animation.
struct Shaped;

impl crate::app::App for Shaped {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        let visits = crate::hot_signal("hydration::visits", 3u32);
        let entrance = motion_core::Animated::new(
            0.0f32,
            motion_core::tween(
                std::time::Duration::from_millis(400),
                motion_core::Easing::Linear,
            ),
        );
        entrance.retarget(1.0);
        let mut children: Vec<Box<dyn ui_core::LayoutItem>> = Vec::new();
        if preferences_core::use_color_scheme() == Some(ColorScheme::Dark) {
            let banner = ui_core::Container::new(layout_core::LayoutStyle::new(), Vec::new())
                .unwrap()
                .role(renderer_core::Role::Banner);
            children.push(Box::new(banner));
        }
        let line = ui_core::Text::new(
            move || format!("visits {} at {}", visits.get(), entrance.get()),
            layout_core::LayoutStyle::new(),
            || renderer_core::TextStyle::new(16.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        children.push(Box::new(line));
        let main = ui_core::Container::new(layout_core::LayoutStyle::new().flex_column(), children)
            .unwrap()
            .role(renderer_core::Role::Main);
        Box::new(ui_core::WindowRoot::new(Box::new(main)))
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        match preferences_core::use_color_scheme() {
            Some(ColorScheme::Dark) => Some(renderer_core::Color::BLACK),
            _ => Some(renderer_core::Color::WHITE),
        }
    }
}

fn main_id(commands: &[renderer_core::DrawCommand]) -> Option<u64> {
    commands.iter().find_map(|command| match command {
        renderer_core::DrawCommand::PushElement { element }
            if element.semantics.role == renderer_core::Role::Main =>
        {
            Some(element.id.0)
        }
        _ => None,
    })
}

fn texts(commands: &[renderer_core::DrawCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            renderer_core::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_first_tree_is_built_from_the_page_and_then_follows_the_browser() {
    let request = telar_project::PrerenderRequest {
        out: std::env::temp_dir().join("telar-hydration-flow.json"),
        page: telar_project::PageRequest::At {
            location: telar_project::PageLocation::root(),
        },
        surface: Surface {
            width: WIDTH,
            height: HEIGHT,
        },
        preferences: Preferences {
            color_scheme: Some("dark".to_string()),
            ..Preferences::default()
        },
        base: "/".to_string(),
    };
    let page = crate::runner::prerender_page(
        crate::app_config::AppConfig::default(),
        Shaped,
        "telar-hydration-test",
        &request,
    );
    let written_main = page
        .markup
        .split("<main data-telar-id=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next()?.parse::<u64>().ok())
        .expect("the page has its main box");
    let mut state = page.state.clone();
    state
        .signals
        .insert("hydration::visits".to_string(), "7".to_string());
    let hydration = Hydration::new(
        state.version,
        platform_core::SystemPreferences {
            color_scheme: Some(ColorScheme::Dark),
            ..platform_core::SystemPreferences::default()
        },
        (state.surface.width, state.surface.height),
        state.locale,
        state.signals.into_iter().collect(),
    )
    .expect("a state this client reads");

    let recording = renderer_record::Recording::new();
    let was = ui_tree::set_element_capture(true);
    super::super::generic::run_on_platform::<_, Shaped, ()>(
        platform_headless::HeadlessPlatform::new(WIDTH, HEIGHT)
            .with_location([platform_core::Location::root()])
            .with_system_preferences(platform_core::SystemPreferences {
                color_scheme: Some(ColorScheme::Light),
                ..platform_core::SystemPreferences::default()
            })
            .with_frames(2),
        crate::app_config::AppConfig::default(),
        std::sync::Arc::new(services_core::NoPaths),
        Shaped,
        "telar-hydration-test",
        super::super::host::SurfaceRenderer::installed(renderer_record::RecordingFactory::new(
            recording.clone(),
        )),
        None,
        Some(hydration),
    )
    .expect("headless run failed");
    ui_tree::set_element_capture(was);

    let first = recording
        .frames()
        .into_iter()
        .next()
        .expect("a first frame");
    assert_eq!(
        main_id(&first.commands),
        Some(written_main),
        "built under the scheme the page assumed, the tree opens the nodes the page names"
    );
    assert_eq!(
        first.clear,
        Some(renderer_core::Color::WHITE),
        "the scheme this browser reports arrives before the first frame"
    );
    assert_eq!(
        texts(&first.commands),
        ["visits 7 at 1"],
        "the keyed signal is the page's and the entrance has already played"
    );
}
