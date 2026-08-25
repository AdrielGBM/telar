//! The `[telar.dev.window]` overrides `cargo telar dev` passes through the environment.
//!
//! Their own module rather than desktop's: they read env vars into a `WindowConfig` and touch no winit, and
//! the android runner called them as `super::desktop::…` while `mod desktop` is gated off Android — an
//! unresolved module the moment a hot-reload build targeted it.

use crate::app_config::AppConfig;

/// Applies the dev-window overrides to `config`, and returns it unchanged in a build that is not a
/// hot-reload one — so every boot path calls this the same way and the `cfg` lives in one place.
///
/// **Precedence, deliberately**: these land on the config the caller passes, which
/// [`super::resolved_window`] then lets [`crate::App::window_config`] replace outright. The app's own answer
/// wins, and trinity's `[telar.dev.window]` depends on that order. Pinned by the test below.
pub(super) fn with_dev_overrides(config: AppConfig) -> AppConfig {
    #[cfg(not(telar_hot_reload))]
    return config;
    #[cfg(telar_hot_reload)]
    {
        let AppConfig {
            mut window,
            font_paths,
            font_data,
            font_family,
        } = config;
        apply_dev_window_overrides(&mut window);
        AppConfig {
            window,
            font_paths,
            font_data,
            font_family,
        }
    }
}

#[cfg(telar_hot_reload)]
fn apply_dev_window_overrides(config: &mut platform_core::WindowConfig) {
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_TITLE") {
        config.title = v;
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_WIDTH") {
        if let Ok(n) = v.parse() {
            config.width = n;
        }
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_HEIGHT") {
        if let Ok(n) = v.parse() {
            config.height = n;
        }
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_DECORATIONS") {
        config.has_decorations = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_RESIZABLE") {
        config.is_resizable = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_TRANSPARENT") {
        config.is_transparent = v == "1";
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_FULLSCREEN") {
        config.fullscreen = match v.as_str() {
            "borderless" => platform_core::FullscreenMode::Borderless,
            "exclusive" => platform_core::FullscreenMode::Exclusive,
            _ => platform_core::FullscreenMode::Disabled,
        };
    }
    if let Ok(v) = std::env::var("TELAR_DEV_WINDOW_POSITION") {
        config.position = parse_dev_window_position(&v);
    }
}

// Parses the TELAR_DEV_WINDOW_POSITION value: "centered" (or empty/invalid) → Centered; "<x>,<y>" → absolute coordinates.
#[cfg(telar_hot_reload)]
fn parse_dev_window_position(value: &str) -> platform_core::WindowPosition {
    let value = value.trim();
    if let Some((x, y)) = value.split_once(',')
        && let (Ok(x), Ok(y)) = (x.trim().parse::<i32>(), y.trim().parse::<i32>())
    {
        return platform_core::WindowPosition::At(x, y);
    }
    platform_core::WindowPosition::Centered
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::runner::resolved_window;

    // The order the two window sources resolve in, pinned so a boot-path merge cannot flip it in silence:
    // the dev overrides land on the caller's config, and `App::window_config` replaces it outright.
    // trinity's `[telar.dev.window]` depends on exactly this — an app that answers `None` keeps the
    // overridden window, and one that answers `Some` overrides them back.
    #[test]
    fn the_app_window_config_wins_over_the_dev_overrides() {
        struct Opinionated;
        impl App for Opinionated {
            fn root(&self) -> Box<dyn ui_tree::Component> {
                unreachable!("window resolution never builds the tree")
            }
            fn window_config(&self) -> Option<platform_core::WindowConfig> {
                Some(platform_core::WindowConfig {
                    title: "from the app".to_string(),
                    ..Default::default()
                })
            }
        }
        struct Indifferent;
        impl App for Indifferent {
            fn root(&self) -> Box<dyn ui_tree::Component> {
                unreachable!("window resolution never builds the tree")
            }
        }

        let dev_window = platform_core::WindowConfig {
            title: "from cargo telar dev".to_string(),
            ..Default::default()
        };
        assert_eq!(
            resolved_window(dev_window.clone(), &Opinionated).title,
            "from the app"
        );
        assert_eq!(
            resolved_window(dev_window, &Indifferent).title,
            "from cargo telar dev"
        );
    }
}
