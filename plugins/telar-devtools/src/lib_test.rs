use super::*;

fn chord(key: char) -> Event {
    Event::KeyPressed {
        key: Key::Char(key),
        modifiers: ModifiersState {
            is_ctrl: true,
            is_shift: true,
            ..ModifiersState::default()
        },
    }
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: platform_core::PointerButton::Primary,
        source: platform_core::PointerSource::Mouse,
    }
}

fn open_panel(dev: &mut DevTools) {
    dev.on_event(&chord('d'));
}

#[test]
fn the_shortcuts_ask_the_runner_and_leave_the_key_to_the_app() {
    let mut dev = DevTools::default();
    assert_eq!(
        dev.on_event(&chord('b')),
        OverlayResponse {
            consumed: false,
            action: Some(DevAction::ToggleBackend),
        }
    );
    assert_eq!(
        dev.on_event(&chord('i')).action,
        Some(DevAction::Redraw),
        "toggling the inspector redraws"
    );
    assert!(dev.inspector_open);
    let plain = Event::KeyPressed {
        key: Key::Char('i'),
        modifiers: ModifiersState::default(),
    };
    assert_eq!(dev.on_event(&plain), OverlayResponse::IGNORED);
}

#[test]
fn a_press_on_the_badge_is_kept_from_the_app_and_one_elsewhere_is_not() {
    let mut dev = DevTools::default();
    drop(dev.on_frame(&[], 800.0, 600.0, false));
    assert_eq!(dev.on_event(&press(10.0, 10.0)), OverlayResponse::IGNORED);
    let badge = dev.on_event(&press(750.0, 578.0));
    assert!(badge.consumed, "the badge press is the overlay's");
    assert_eq!(badge.action, Some(DevAction::Redraw));
    assert!(dev.panel_open, "and it opens the panel");
}

#[test]
fn the_fps_counter_keeps_frames_coming_while_the_app_is_idle() {
    assert!(DevTools::default().needs_frame());
}

fn panel_text(dev: &mut DevTools) -> Vec<String> {
    dev.on_frame(&[], 800.0, 600.0, false)
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

// The panel offers `ctrl+shift+b toggle renderer` one line below this one, so a reader who cannot see which backend is live is being asked to toggle blind. Nothing called `set_renderer_info`, so the line never drew.
#[test]
fn the_panel_names_the_backend_that_is_drawing() {
    let mut dev = DevTools::default();
    open_panel(&mut dev);
    dev.set_renderer_info("software");
    assert!(
        panel_text(&mut dev)
            .iter()
            .any(|t| t == "renderer: software"),
        "the panel must name the live backend"
    );
}

#[test]
fn the_backend_line_follows_a_switch() {
    let mut dev = DevTools::default();
    open_panel(&mut dev);
    dev.set_renderer_info("software");
    dev.set_renderer_info("hardware (wgpu)");
    let text = panel_text(&mut dev);
    assert!(
        text.iter().any(|t| t == "renderer: hardware (wgpu)"),
        "the overlay must name the backend in use: {text:?}"
    );
    assert!(
        !text.iter().any(|t| t == "renderer: software"),
        "and not the one it switched away from: {text:?}"
    );
}
