use std::sync::Arc;

use platform_web::{WebClipboard, WebPlatform, WebPlatformConfig};
use renderer_web::{WebGpuRendererFactory, canvas_in};
use services_core::{AppPathsProvider, NoPaths};

use crate::app::App;
use crate::app_config::AppConfig;

/// How a browser app is mounted. Everything here is a property of the *page*, not of the application, which
/// is why none of it lives in [`AppConfig`].
#[derive(Clone, Debug, Default)]
pub struct WebOptions {
    /// A CSS selector for the element the app fills. `None` mounts on `<body>`.
    pub host: Option<String>,
    /// Whether the app takes the keyboard once mounted, and whether it claims touch gestures inside its
    /// host. Both on by default; an app embedded in a scrolling page turns the second off.
    pub focus_and_gestures: Option<(bool, bool)>,
}

/// Mounts an app on the page and returns, leaving it running on the browser's animation-frame loop.
///
/// There is no filesystem behind [`NoPaths`], which is deliberate rather than a stub: the preferences file
/// and the font directories a desktop build reads have no browser equivalent, and a provider that answered
/// with paths nothing can open would only move the failure later. A web app's fonts come from
/// [`AppConfig::font_data`] — bytes linked into the binary — because the browser's own faces are not
/// something a glyph shaper can reach.
pub fn run_web_app_with_name<A: App>(
    config: AppConfig,
    options: WebOptions,
    app: A,
    app_name: &str,
) {
    platform_web::install_console_logging();

    let (autofocus, owns_gestures) = options.focus_and_gestures.unwrap_or((true, true));
    let host = match platform_web::host_element(options.host.as_deref()) {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("telar could not find the element to mount on: {e}");
            return;
        }
    };
    let canvas = match canvas_in(&host) {
        Ok(canvas) => canvas,
        Err(e) => {
            tracing::error!("telar could not create a canvas to draw on: {e}");
            return;
        }
    };

    services_core::set_clipboard(Arc::new(WebClipboard::new()));
    let paths: Arc<dyn AppPathsProvider> = Arc::new(NoPaths);
    let platform = WebPlatform::with_host(
        host,
        WebPlatformConfig {
            host: options.host,
            autofocus,
            owns_gestures,
        },
    );

    if let Err(e) = super::run_with_platform_and_renderer::<_, _, A, ()>(
        platform,
        WebGpuRendererFactory::new(canvas),
        config,
        paths,
        app,
        app_name,
    ) {
        tracing::error!("telar could not start: {e}");
    }
}
