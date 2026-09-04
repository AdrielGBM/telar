//! Starting an app in the browser.

use std::sync::Arc;

use platform_web::{WebClipboard, WebPlatform, WebPlatformConfig};
use renderer_web::{WebGpuRendererFactory, canvas_in};
use services_core::{AppPathsProvider, NoPaths};

use crate::app::App;
use crate::app_config::AppConfig;

/// How a browser app is mounted. Everything here is a property of the *page*, not of the application, which is why none of it lives in [`AppConfig`].
#[derive(Clone, Debug, Default)]
pub struct WebOptions {
    /// A CSS selector for the element the app fills. `None` mounts on `<body>`.
    pub host: Option<String>,
    /// Whether the app takes the keyboard once mounted, and whether it claims touch gestures inside its host. Both on by default; an app embedded in a scrolling page turns the second off.
    pub focus_and_gestures: Option<(bool, bool)>,
    /// Which renderer to use, when the page wants to decide rather than let the browser decide for it.
    pub renderer: WebRenderer,
    /// Whether a right-click opens the application's own menu instead of the browser's.
    ///
    /// `None`, the default, lets the way the app draws decide. Pixels on a canvas give the browser's menu nothing to act on — no text to copy, no link to open, no image to save — so there the app takes it. A document has all three, and an app that swallowed the menu there would be a page missing the one thing every other page has. An application that draws its own menus over a document says so here.
    pub owns_context_menu: Option<bool>,
}

/// How a browser app draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WebRenderer {
    /// Pixels on a canvas where the browser offers a GPU adapter, and a document where it does not.
    ///
    /// The default, and not a convenience: on Linux, Chrome draws WebGPU through Vulkan, and Vulkan is off in many driver and distribution combinations. A page that could only draw pixels would be blank for those people, through nothing the application did.
    #[default]
    Auto,
    /// Pixels, or nothing. For an app whose frame is a picture rather than an interface.
    Canvas,
    /// Real elements, laid out by CSS. Text that can be selected and found, a tree a screen reader can walk, native focus and a working input method — and no GPU at all.
    Document,
}

impl WebRenderer {
    /// The choice, once the page has had its say.
    ///
    /// A document is not a fallback for a canvas; it is the other way of drawing an interface, and which one a build uses is a decision the page can make and a link can override — `?telar-renderer=dom` — without rebuilding anything. An application that named one keeps it: `Auto` is what "let somebody else decide" is spelled as, so only it asks.
    fn resolved(self, host: &web_sys::HtmlElement) -> Self {
        if self != Self::Auto {
            return self;
        }
        match platform_web::page_setting(host, "renderer")
            .as_deref()
            .map(str::trim)
        {
            Some("dom" | "document") => Self::Document,
            Some("canvas" | "gpu" | "webgpu") => Self::Canvas,
            Some(other) if !other.is_empty() && other != "auto" => {
                tracing::warn!(
                    "`{other}` is not a renderer this build has; the choice is `dom`, `canvas` or `auto`"
                );
                Self::Auto
            }
            _ => Self::Auto,
        }
    }
}

/// Mounts an app on the page and returns, leaving it running on the browser's animation-frame loop.
///
/// There is no filesystem behind [`NoPaths`], which is deliberate rather than a stub: the preferences file and the font directories a desktop build reads have no browser equivalent, and a provider that answered with paths nothing can open would only move the failure later.
pub fn run_web_app_with_name<A: App>(
    config: AppConfig,
    options: WebOptions,
    app: A,
    app_name: &str,
) {
    platform_web::install_console_logging();
    // Owned, because the choice below is made after a promise settles and this call has long returned.
    let app_name = app_name.to_string();

    let host = match platform_web::host_element(options.host.as_deref()) {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("telar could not find the element to mount on: {e}");
            return;
        }
    };

    services_core::set_clipboard(Arc::new(WebClipboard::new()));

    // The choice has to be made before the app is built: it decides what measures text, and text is measured while the tree is being built. Asking the browser costs one promise, and answering wrong costs a page that never draws.
    let wanted = options.renderer.resolved(&host);
    let host_for_start = host.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let document = match wanted {
            WebRenderer::Document => true,
            WebRenderer::Canvas => false,
            WebRenderer::Auto => match renderer_web::webgpu_available().await {
                Ok(()) => false,
                Err(reason) => {
                    tracing::info!("drawing as a document: {}", reason.message());
                    true
                }
            },
        };
        start(config, options, host_for_start, document, app, &app_name);
    });
}

fn start<A: App>(
    config: AppConfig,
    options: WebOptions,
    host: web_sys::HtmlElement,
    document: bool,
    app: A,
    app_name: &str,
) {
    let (autofocus, owns_gestures) = options.focus_and_gestures.unwrap_or((true, true));
    let paths: Arc<dyn AppPathsProvider> = Arc::new(NoPaths);
    let platform_config = WebPlatformConfig {
        host: options.host,
        autofocus,
        owns_gestures,
        // The boxes that scroll are the document's own, and a wheel the app claimed would be a wheel the compositor never sees.
        owns_scroll: document,
        owns_context_menu: options.owns_context_menu.unwrap_or(!document),
    };
    let platform = WebPlatform::with_host(host.clone(), platform_config);

    let result = if document {
        // Boxes are placed by CSS from what each one declared, so what measures text has to be the engine that will draw it — and the elements have to carry their declarations at all.
        renderer_core::set_text_metrics(renderer_dom::CanvasTextMetrics);
        ui_tree::set_element_capture(true);
        crate::runner::run_with_platform_and_renderer::<_, _, A, ()>(
            platform,
            renderer_dom::DomRendererFactory::new(host),
            config,
            paths,
            app,
            app_name,
        )
    } else {
        let canvas = match canvas_in(&host) {
            Ok(canvas) => canvas,
            Err(e) => {
                tracing::error!("telar could not create a canvas to draw on: {e}");
                return;
            }
        };
        crate::runner::run_with_platform_and_renderer::<_, _, A, ()>(
            platform,
            WebGpuRendererFactory::new(canvas),
            config,
            paths,
            app,
            app_name,
        )
    };
    if let Err(e) = result {
        tracing::error!("telar could not start: {e}");
    }
}
