//! The `[telar.dev.window]` overrides `cargo telar dev` passes through the environment.
//!
//! Their own module rather than desktop's: they read env vars into a `WindowConfig` and touch no winit, and the android runner called them as `super::desktop::…` while `mod desktop` is gated off Android — an unresolved module the moment a hot-reload build targeted it.

use crate::app_config::AppConfig;

/// Applies the dev-window overrides to `config`, and returns it unchanged in a build that is not a hot-reload one — so every boot path calls this the same way and the `cfg` lives in one place.
///
/// **Precedence, deliberately**: these land on the config the caller passes, which [`super::resolved_window`] then lets [`crate::App::window_config`] replace outright. The app's own answer wins, and trinity's `[telar.dev.window]` depends on that order. Pinned by the test below.
pub(super) fn with_dev_overrides(config: AppConfig) -> AppConfig {
    #[cfg(not(feature = "hot-reload"))]
    return config;
    #[cfg(feature = "hot-reload")]
    {
        let AppConfig {
            mut window,
            fonts: faces,
            font_family,
        } = config;
        apply_dev_window_overrides(&mut window);
        AppConfig {
            window,
            fonts: faces,
            font_family,
        }
    }
}

#[cfg(feature = "hot-reload")]
pub(super) fn apply_dev_window_overrides(config: &mut platform_core::WindowConfig) {
    let overrides = telar_project::protocol::dev_window_from_env(|var| std::env::var(var).ok())
        .unwrap_or_else(|error| {
            panic!("`cargo telar dev` passed a window override this build cannot read: {error}")
        });
    apply(overrides, config);
}

#[cfg(feature = "hot-reload")]
fn apply(overrides: telar_project::WindowSection, config: &mut platform_core::WindowConfig) {
    use telar_project::protocol::{FullscreenMode, WindowPosition};

    let telar_project::WindowSection {
        title,
        width,
        height,
        decorations,
        resizable,
        transparent,
        fullscreen,
        position,
    } = overrides;
    if let Some(title) = title {
        config.title = title;
    }
    if let Some(width) = width {
        config.width = width;
    }
    if let Some(height) = height {
        config.height = height;
    }
    if let Some(decorations) = decorations {
        config.has_decorations = decorations;
    }
    if let Some(resizable) = resizable {
        config.is_resizable = resizable;
    }
    if let Some(transparent) = transparent {
        config.is_transparent = transparent;
    }
    if let Some(fullscreen) = fullscreen {
        config.fullscreen = match fullscreen {
            FullscreenMode::Disabled => platform_core::FullscreenMode::Disabled,
            FullscreenMode::Borderless => platform_core::FullscreenMode::Borderless,
            FullscreenMode::Exclusive => platform_core::FullscreenMode::Exclusive,
        };
    }
    if let Some(position) = position {
        config.position = match position {
            WindowPosition::Centered => platform_core::WindowPosition::Centered,
            WindowPosition::At { x, y } => platform_core::WindowPosition::At(x, y),
        };
    }
}

#[cfg(test)]
#[path = "dev_window.test.rs"]
mod tests;
