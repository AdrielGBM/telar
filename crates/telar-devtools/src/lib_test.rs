use super::*;

fn open_panel(dev: &mut DevTools) {
    dev.on_key(
        &Key::Char('d'),
        ModifiersState {
            is_ctrl: true,
            is_shift: true,
            ..ModifiersState::default()
        },
    );
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
