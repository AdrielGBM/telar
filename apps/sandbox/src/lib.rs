//! Telar's documentation app: one section per feature, each rendered from its own `.rsx`.

// Module tree (core/, shared/) is declared by `telar::app!`.
telar::app!(
    core::theme::SandboxTheme,
    {
        core::theme::register_modes();
        // Open following the OS light/dark preference (modern for light, midnight for dark); the sidebar buttons still override manually until the next OS change.
        telar::follow_system(core::theme::DEFAULT_MODE, "midnight");
        telar::follow_locale_direction();
    },
    app_config(),
    core::app::SandboxRoot
);

/// The faces this app shapes its text in.
///
/// A browser build that shapes its own glyphs has to bring them: there is no font directory behind a page, so a shaper handed nothing finds nothing and every string measures to zero. Every other target reads the system's.
///
/// One that draws as a document shapes nothing — the browser lays the text out in its own fonts — and the face would be three quarters of a megabyte in a module that never opens it.
fn app_config() -> telar::AppConfig {
    #[allow(unused_mut)]
    let mut config = telar::AppConfig::default();
    #[cfg(target_arch = "wasm32")]
    if telar::SHAPES_TEXT {
        config
            .font_data
            .push(include_bytes!("../assets/fonts/DejaVuSans.ttf").to_vec());
        config.font_family = Some("DejaVu Sans".to_string());
    }
    config
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
