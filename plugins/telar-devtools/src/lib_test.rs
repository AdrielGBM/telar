use std::cell::Cell;

use telar::testing::{key_with, moved, named, press, release};
use telar::{Insets, Rect};

use super::*;
use web_time::Instant;

const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 700.0;
const ERROR: &str = "error[E0425]: cannot find value `count` in this scope\n --> src/app.rs:12:9\n  |\n12 |     count + 1\n  |     ^^^^^ not found in this scope";

fn chord(key: char) -> Event {
    let modifiers = ModifiersState {
        is_ctrl: true,
        is_shift: true,
        ..ModifiersState::default()
    };
    key_with(Key::Char(key), modifiers)
}

/// Every string the overlay draws over an empty app, with where it lands.
fn drawn(dev: &mut DevTools) -> Vec<(String, Rect)> {
    let mut found = Vec::new();
    let frame = dev.on_frame(&[], WIDTH, HEIGHT, false);
    telar::for_each_with_matrix(&frame, |command, matrix| {
        if let DrawCommand::Text { text, rect, .. } = command {
            found.push((text.to_string(), telar::transform_clip_rect(matrix, *rect)));
        }
    });
    found
}

fn texts(dev: &mut DevTools) -> Vec<String> {
    drawn(dev).into_iter().map(|(text, _)| text).collect()
}

fn shows(dev: &mut DevTools, needle: &str) -> bool {
    texts(dev).iter().any(|text| text.contains(needle))
}

/// Whether the overlay draws exactly `text`, as one string.
fn draws(dev: &mut DevTools, text: &str) -> bool {
    texts(dev).iter().any(|drawn| drawn == text)
}

fn centre_of(dev: &mut DevTools, needle: &str) -> (f32, f32) {
    let (_, rect) = drawn(dev)
        .into_iter()
        .find(|(text, _)| text == needle)
        .unwrap_or_else(|| panic!("{needle:?} is not drawn"));
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn node(id: u64, name: &'static str, depth: usize, rect: Rect) -> SegmentNodeInfo {
    SegmentNodeInfo {
        id,
        name,
        depth,
        rect,
        padding: Insets::new(4.0, 8.0, 4.0, 8.0),
        margin: Insets::default(),
        border: Insets::new(1.0, 1.0, 1.0, 1.0),
        gap: Size::new(6.0, 0.0),
    }
}

fn walk() -> Vec<SegmentNodeInfo> {
    vec![
        node(0, "AppRoot", 0, Rect::new(0.0, 0.0, WIDTH, HEIGHT)),
        node(1, "Column", 1, Rect::new(500.0, 100.0, 300.0, 200.0)),
        node(2, "SaveButton", 2, Rect::new(520.0, 120.0, 120.0, 32.0)),
        node(3, "Caption", 1, Rect::new(500.0, 320.0, 200.0, 20.0)),
    ]
}

#[test]
fn the_badge_shows_the_frame_rate() {
    let mut dev = devtools();
    dev.on_frame(&[], WIDTH, HEIGHT, true);
    let text = texts(&mut dev);
    assert!(text.iter().any(|t| t == "DEV"), "{text:?}");
    assert!(text.iter().any(|t| t == "1 fps"), "{text:?}");
}

#[test]
fn the_chords_toggle_the_overlay_and_still_reach_the_app() {
    let mut dev = devtools();
    drawn(&mut dev);
    assert_eq!(
        dev.on_event(&chord('b')),
        OverlayResponse {
            consumed: false,
            action: Some(DevAction::ToggleBackend),
        }
    );

    assert!(!shows(&mut dev, "Telar devtools"));
    assert_eq!(
        dev.on_event(&chord('d')),
        OverlayResponse {
            consumed: false,
            action: Some(DevAction::Redraw),
        }
    );
    assert!(
        shows(&mut dev, "Telar devtools"),
        "Ctrl+Shift+D opens the panel"
    );

    assert!(!draws(&mut dev, "0 components"));
    assert!(!dev.on_event(&chord('I')).consumed);
    assert!(
        draws(&mut dev, "0 components"),
        "Ctrl+Shift+I opens the inspector"
    );

    dev.on_event(&chord('d'));
    dev.on_event(&chord('i'));
    assert!(
        !shows(&mut dev, "Telar devtools"),
        "and the chords close them"
    );
    assert!(!draws(&mut dev, "0 components"));

    let plain = key_with(Key::Char('i'), ModifiersState::default());
    assert_eq!(dev.on_event(&plain), OverlayResponse::IGNORED);
}

#[test]
fn the_panel_names_the_backend_that_is_drawing_and_follows_a_switch() {
    let mut dev = devtools();
    dev.on_event(&chord('d'));
    assert!(shows(&mut dev, "Starting"));
    dev.set_renderer_info("software");
    assert!(shows(&mut dev, "software"));
    dev.set_renderer_info("hardware (wgpu)");
    let text = texts(&mut dev);
    assert!(text.iter().any(|t| t == "hardware (wgpu)"), "{text:?}");
    assert!(!text.iter().any(|t| t == "software"), "{text:?}");
}

#[test]
fn the_banner_shows_the_build_error_until_a_build_lands() {
    let mut dev = devtools();
    assert!(!shows(&mut dev, "Build failed"));
    dev.set_build_error(Some(ERROR.to_owned()));
    assert!(shows(&mut dev, "Build failed"));
    assert!(
        shows(&mut dev, "cannot find value `count` in this scope"),
        "the compiler's report is shown"
    );
    assert!(shows(&mut dev, "src/app.rs:12:9"));

    dev.set_build_error(None);
    assert!(!shows(&mut dev, "Build failed"), "a landed build clears it");
}

#[test]
fn the_inspector_lists_the_mounted_components_nested() {
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.on_event(&chord('i'));
    let text = texts(&mut dev);
    for name in ["AppRoot", "Column", "SaveButton", "Caption", "4 components"] {
        assert!(text.iter().any(|t| t == name), "{name} is listed: {text:?}");
    }
    let (_, column) = centre_of(&mut dev, "Column");
    let (_, button) = centre_of(&mut dev, "SaveButton");
    let (_, caption) = centre_of(&mut dev, "Caption");
    assert!(
        column < button && button < caption,
        "in walk order, every branch open"
    );
    assert!(shows(&mut dev, "Select a component to see its box."));
}

#[test]
fn choosing_a_component_shows_its_box_and_marks_it_over_the_app() {
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.on_event(&chord('i'));
    let (x, y) = centre_of(&mut dev, "SaveButton");
    assert!(dev.on_event(&press(x, y)).consumed);
    dev.on_event(&release(x, y));

    let text = texts(&mut dev);
    for fact in ["520, 120", "120 × 32", "4 8 4 8", "6 × 0", "2 · #2"] {
        assert!(text.iter().any(|t| t == fact), "{fact} is shown: {text:?}");
    }
    let frame = dev.on_frame(&[], WIDTH, HEIGHT, false);
    let marked = frame.iter().any(|command| {
        matches!(command, DrawCommand::Rect { rect, .. } if *rect == Rect::new(520.0, 120.0, 120.0, 32.0))
    });
    assert!(marked, "the chosen component is outlined over the app");
}

#[test]
fn a_press_on_the_badge_is_the_overlays_and_one_elsewhere_is_the_apps() {
    let mut dev = devtools();
    drawn(&mut dev);
    assert_eq!(
        dev.on_event(&press(WIDTH / 2.0, HEIGHT / 2.0)),
        OverlayResponse::IGNORED
    );

    let (x, y) = centre_of(&mut dev, "DEV");
    let badge = dev.on_event(&press(x, y));
    assert!(badge.consumed, "the badge press is the overlay's");
    assert_eq!(badge.action, Some(DevAction::Redraw));
    assert!(
        dev.on_event(&release(x, y)).consumed,
        "and so is its release"
    );
    assert!(shows(&mut dev, "Telar devtools"), "and it opens the panel");

    let (x, y) = centre_of(&mut dev, "Telar devtools");
    assert!(
        dev.on_event(&press(x, y)).consumed,
        "a press on the panel too"
    );
    dev.on_event(&release(x, y));
    assert!(dev.on_event(&moved(x, y)).consumed, "and a move over it");
    assert!(!dev.on_event(&moved(WIDTH / 2.0, HEIGHT / 2.0)).consumed);
}

#[test]
fn the_inspector_takes_presses_on_its_drawer_and_leaves_the_rest_of_the_window() {
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.on_event(&chord('i'));
    let (x, y) = centre_of(&mut dev, "4 components");
    assert!(dev.on_event(&press(x, y)).consumed, "over the drawer");
    dev.on_event(&release(x, y));
    assert!(
        !dev.on_event(&press(WIDTH * 0.75, HEIGHT / 2.0)).consumed,
        "beside it the app is pressed"
    );
}

#[test]
fn the_keyboard_reaches_the_overlay_only_while_a_panel_holds_focus() {
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.on_event(&chord('i'));
    let down = named(telar::NamedKey::ArrowDown);
    assert!(
        !dev.on_event(&down).consumed,
        "nothing of the overlay's is focused"
    );

    let (x, y) = centre_of(&mut dev, "Column");
    dev.on_event(&press(x, y));
    dev.on_event(&release(x, y));
    drawn(&mut dev);
    assert!(dev.on_event(&down).consumed, "the tree walks its rows");

    dev.on_event(&press(WIDTH * 0.75, HEIGHT / 2.0));
    drawn(&mut dev);
    assert!(
        !dev.on_event(&down).consumed,
        "a press on the app hands the keyboard back"
    );
}

#[test]
fn a_change_to_the_overlay_asks_for_a_frame_of_its_own() {
    let mut dev = devtools();
    drawn(&mut dev);
    assert!(!dev.is_dirty(), "a composed overlay has nothing new");
    dev.on_event(&chord('d'));
    assert!(dev.is_dirty(), "an opened panel has to be drawn");
    drawn(&mut dev);
    assert!(!dev.is_dirty());
}

#[test]
fn the_fps_counter_keeps_frames_coming_while_the_app_is_idle() {
    assert!(devtools().needs_frame());
}

#[test]
fn the_overlay_is_drawn_over_the_app() {
    let mut dev = devtools();
    let app = [DrawCommand::PopClip];
    let frame = dev.on_frame(&app, WIDTH, HEIGHT, true);
    assert!(matches!(frame[0], DrawCommand::PopClip), "the app first");
    assert!(frame.len() > app.len(), "then the overlay");
}

thread_local! {
    static NOW: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn frozen_now() -> Instant {
    NOW.with(|now| match now.get() {
        Some(at) => at,
        None => {
            let at = Instant::now();
            now.set(Some(at));
            at
        }
    })
}

fn devtools() -> DevTools {
    telar::install_default_text_metrics();
    DevTools {
        clock: Clock::reading(frozen_now),
        ..DevTools::default()
    }
}

#[test]
fn the_banner_and_the_inspector_drawer_share_the_window_without_covering_each_other() {
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.set_build_error(Some(ERROR.to_owned()));
    let (alone, _) = centre_of(&mut dev, "Build failed");

    dev.on_event(&chord('i'));
    let found = drawn(&mut dev);
    let banner = found
        .iter()
        .find(|(text, _)| text == "Build failed")
        .map(|(_, rect)| *rect)
        .expect("the banner stays while the inspector is open");
    let drawer_edge = found
        .iter()
        .filter(|(text, _)| ["Inspector", "4 components", "Column"].contains(&text.as_str()))
        .map(|(_, rect)| rect.x + rect.width)
        .fold(0.0, f32::max);
    assert!(drawer_edge > 0.0, "the drawer is drawn");
    assert!(
        banner.x >= drawer_edge,
        "the banner starts right of the drawer's content: {} < {drawer_edge}",
        banner.x
    );
    assert!(banner.x > alone, "and moved over to make the room");
    assert_eq!(
        found
            .iter()
            .filter(|(text, _)| text == "Build failed")
            .count(),
        1,
        "one banner, not one per place"
    );

    dev.on_event(&chord('i'));
    let (back, _) = centre_of(&mut dev, "Build failed");
    assert_eq!(
        back, alone,
        "closing the inspector gives the banner its width back"
    );
}

#[test]
fn the_stats_and_the_inspector_drawer_share_the_window_without_covering_each_other() {
    const NARROW: f32 = 560.0;
    let mut dev = devtools();
    dev.on_tree(&walk());
    dev.on_event(&chord('d'));
    dev.on_event(&chord('i'));

    let mut found = Vec::new();
    let frame = dev.on_frame(&[], NARROW, HEIGHT, false);
    telar::for_each_with_matrix(&frame, |command, matrix| {
        if let DrawCommand::Text { text, rect, .. } = command {
            found.push((text.to_string(), telar::transform_clip_rect(matrix, *rect)));
        }
    });
    let left_of = |name: &str| {
        found
            .iter()
            .find(|(text, _)| text == name)
            .map(|(_, rect)| rect.x)
            .unwrap_or_else(|| panic!("{name:?} is not drawn"))
    };
    // "Inspector" is left out: the stats panel has a row with that label too.
    let drawer_edge = found
        .iter()
        .filter(|(text, _)| ["4 components", "Close", "Column"].contains(&text.as_str()))
        .map(|(_, rect)| rect.x + rect.width)
        .fold(0.0, f32::max);
    assert!(drawer_edge > 0.0, "the drawer is drawn");
    for name in ["DEV", "Telar devtools"] {
        assert!(
            left_of(name) >= drawer_edge,
            "{name:?} starts at {} inside the drawer, which ends at {drawer_edge}",
            left_of(name)
        );
    }
    assert_eq!(
        found.iter().filter(|(text, _)| text == "DEV").count(),
        1,
        "one badge, not one per place"
    );
}
