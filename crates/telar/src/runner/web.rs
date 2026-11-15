//! Starting an app in the browser.

use std::sync::Arc;

use platform_web::{PageNames, WebClipboard, WebPlatform, WebPlatformConfig};
#[cfg(feature = "web")]
use renderer_web::{WebGpuRendererFactory, canvas_in};
use services_core::{AppPathsProvider, NoPaths};
use telar_project::{ASSETS_META_NAME, HOST_ELEMENT_ID, PageRenderer, RENDERER_SETTING};

use crate::app::App;
use crate::app_config::AppConfig;

/// How a browser app is mounted. Everything here is a property of the *page*, not of the application, which is why none of it lives in [`AppConfig`].
#[derive(Clone, Debug, Default)]
pub struct WebOptions {
    /// A CSS selector for the element the app fills. `None` mounts on `<body>`.
    pub host: Option<String>,
    /// Whether the app takes the keyboard once mounted, and whether it claims touch gestures inside its host. Both on by default; an app embedded in a scrolling page turns the second off.
    pub focus_and_gestures: Option<(bool, bool)>,
    /// Whether a right-click opens the application's own menu instead of the browser's.
    ///
    /// `None`, the default, lets the way the app draws decide. Pixels on a canvas give the browser's menu nothing to act on — no text to copy, no link to open, no image to save — so there the app takes it. A document has all three, and an app that swallowed the menu there would be a page missing the one thing every other page has. An application that draws its own menus over a document says so here.
    pub owns_context_menu: Option<bool>,
    /// How the app's locations are written as this page's addresses — the base path the site is served under, and whether a location ends in `/`. `None` takes the base from the page's `<base href>`, and `/` where it has none.
    pub location: Option<platform_core::LocationFormat>,
}

/// The names the page `cargo telar` packages gives what the app looks up in it.
fn page_names() -> PageNames {
    PageNames {
        host_id: Some(HOST_ELEMENT_ID),
        assets_meta: Some(ASSETS_META_NAME),
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
    let host = match platform_web::host_element(options.host.as_deref(), page_names().host_id) {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("telar could not find the element to mount on: {e}");
            return;
        }
    };

    services_core::set_clipboard(Arc::new(WebClipboard::new()));
    services_core::set_uri_opener(Arc::new(platform_web::WebUriOpener));

    let document = draws_as_document(page_renderer(&host));
    start(config, options, host, document, app, app_name);
}

/// How the page asks the app to draw: a document unless its renderer setting names the canvas, which a link can set with `?telar-renderer=canvas` as the page can with an attribute.
fn page_renderer(host: &web_sys::HtmlElement) -> PageRenderer {
    let setting = platform_web::page_setting(host, RENDERER_SETTING);
    PageRenderer::from_setting(setting.as_deref()).unwrap_or_else(|error| {
        tracing::warn!(
            "the page's {RENDERER_SETTING} setting is not one this build reads: {error}"
        );
        PageRenderer::default()
    })
}

#[cfg(feature = "web")]
fn draws_as_document(wanted: PageRenderer) -> bool {
    wanted == PageRenderer::Dom
}

#[cfg(not(feature = "web"))]
fn draws_as_document(wanted: PageRenderer) -> bool {
    if wanted == PageRenderer::Canvas {
        tracing::warn!(
            "this build draws as a document; `canvas` needs the `web` feature, where `web-dom` is the document alone"
        );
    }
    true
}

reactive_core::dropless_thread_local! {
    // An effect stops only with its owner, so the follower gets a scope of its own that a re-call disposes; a hot reload re-runs the app's setup.
    static FOLLOW_LANGUAGE: std::cell::Cell<Option<reactive_core::OwnerId>> =
        const { std::cell::Cell::new(None) };
}

/// Keeps `<html lang>`/`<html dir>` matching the active locale, so switching locale relabels the whole
/// document the way `follow_locale_direction` relabels the layout — the same rule, `i18n_core::Direction::for_locale`, applied to the page's own root rather than to a widget's.
fn follow_locale_language() {
    if let Some(previous) = FOLLOW_LANGUAGE.take() {
        reactive_core::dispose_owner(previous);
    }
    let scope = reactive_core::detached(reactive_core::owner_scope);
    reactive_core::effect(|| {
        let lang = i18n_core::use_locale().unwrap_or_else(|| i18n_core::default_locale().clone());
        let is_rtl = i18n_core::Direction::for_locale(&lang).is_rtl();
        platform_web::set_document_language(&lang, is_rtl);
    });
    FOLLOW_LANGUAGE.set(Some(scope.id()));
}

/// The inputs of the prerendered page the host was served with, for a document that takes it over. A canvas does not: it replaces the page once it has drawn.
fn served_page() -> Option<super::hydration::Hydration> {
    super::hydration::Hydration::served()
}

fn start<A: App>(
    config: AppConfig,
    options: WebOptions,
    host: web_sys::HtmlElement,
    document: bool,
    app: A,
    app_name: &str,
) {
    follow_locale_language();
    let (autofocus, owns_gestures) = options.focus_and_gestures.unwrap_or((true, true));
    let paths: Arc<dyn AppPathsProvider> = Arc::new(NoPaths);
    let platform_config = WebPlatformConfig {
        host: options.host,
        autofocus,
        owns_gestures,
        // The boxes that scroll are the document's own, and a wheel the app claimed would be a wheel the compositor never sees.
        owns_scroll: document,
        owns_context_menu: options.owns_context_menu.unwrap_or(!document),
        owns_keyboard: !document,
        location: options.location,
        page: page_names(),
    };
    let platform = WebPlatform::with_host(host.clone(), platform_config);

    let result = if document {
        // Boxes are placed by CSS from what each one declared, so the elements have to carry their declarations at all; what measures their text is the renderer's, the engine that will draw it.
        renderer_dom::remeasure_on_font_load();
        ui_tree::set_element_capture(true);
        super::generic::run_on_platform::<_, A, ()>(
            platform,
            config,
            paths,
            app,
            app_name,
            super::host::SurfaceRenderer::installed(renderer_dom::DomRendererFactory::new(host)),
            None,
            served_page(),
        )
    } else {
        #[cfg(feature = "web")]
        {
            let canvas = match canvas_in(&host) {
                Ok(canvas) => canvas,
                Err(e) => {
                    tracing::error!("telar could not create a canvas to draw on: {e}");
                    return;
                }
            };
            renderer_web::load_page_fonts(telar_project::FONT_FAMILY_ATTRIBUTE);
            crate::runner::run_with_platform_and_renderer::<_, _, A, ()>(
                described::DescribedCanvas::new(platform, canvas.clone()),
                WebGpuRendererFactory::new(canvas),
                config,
                paths,
                app,
                app_name,
            )
        }
        // `draws_as_document` answers unconditionally in a build with no canvas renderer, so this arm is one nothing reaches.
        #[cfg(not(feature = "web"))]
        unreachable!("a build without the canvas renderer draws as a document")
    };
    if let Err(e) = result {
        tracing::error!("telar could not start: {e}");
    }
}

/// A canvas told to assistive technology. Only a build that carries the canvas has it: a document is already what a reader reads, and the page loop drives its handler without ever asking it for a snapshot.
#[cfg(feature = "web")]
mod described {
    use platform_core::{AccessAction, AccessNode, Event, EventHandler, Platform, PlatformError};
    use platform_web::{WebPlatform, WebWindow};
    use renderer_web::CanvasSurface;

    /// The browser platform, driving a handler whose every frame the canvas's mirror then describes.
    pub(super) struct DescribedCanvas {
        platform: WebPlatform,
        canvas: CanvasSurface,
    }

    impl DescribedCanvas {
        pub(super) fn new(platform: WebPlatform, canvas: CanvasSurface) -> Self {
            Self { platform, canvas }
        }
    }

    impl Platform for DescribedCanvas {
        type Window = WebWindow;

        fn location_source(&mut self) -> Option<Box<dyn platform_core::LocationSource>> {
            self.platform.location_source()
        }

        fn keyboard_convention(&self) -> platform_core::KeyboardConvention {
            self.platform.keyboard_convention()
        }

        fn focus_boundary(&self) -> platform_core::FocusBoundary {
            self.platform.focus_boundary()
        }

        fn run<H: EventHandler<WebWindow> + 'static>(
            self,
            config: platform_core::WindowConfig,
            handler: H,
        ) -> Result<(), PlatformError> {
            self.platform.run(
                config,
                Describing {
                    handler,
                    canvas: self.canvas,
                },
            )
        }
    }

    /// `handler`, with the canvas describing the window after each frame from the snapshot the desktop hands its reader. What a reader does to the description comes back through the platform's events, as [`Event::AccessAction`].
    struct Describing<H> {
        handler: H,
        canvas: CanvasSurface,
    }

    impl<H: EventHandler<WebWindow>> EventHandler<WebWindow> for Describing<H> {
        fn on_resume(&mut self, window: &WebWindow) -> bool {
            self.handler.on_resume(window)
        }

        fn on_event(&mut self, event: Event, window: &WebWindow) {
            self.handler.on_event(event, window);
        }

        fn on_redraw(&mut self, window: &WebWindow) {
            self.handler.on_redraw(window);
            self.canvas.describe(&self.handler.accessibility());
        }

        fn on_suspend(&mut self) {
            self.handler.on_suspend();
        }

        fn accessibility(&self) -> Vec<AccessNode> {
            self.handler.accessibility()
        }

        fn root_language(&self) -> Option<platform_core::LocaleTag> {
            self.handler.root_language()
        }

        fn on_accessibility_action(&mut self, id: u64, action: AccessAction) {
            self.handler.on_accessibility_action(id, action);
        }

        fn on_back(&mut self, window: &WebWindow) -> bool {
            self.handler.on_back(window)
        }

        fn remount(&mut self, window: &WebWindow) {
            self.handler.remount(window);
        }

        fn owe_presentation(&mut self) {
            self.handler.owe_presentation();
        }

        fn new_events(&mut self) {
            self.handler.new_events();
        }

        fn about_to_wait(&mut self) -> Option<std::time::Duration> {
            self.handler.about_to_wait()
        }

        fn take_exit_request(&mut self) -> bool {
            self.handler.take_exit_request()
        }

        fn last_frame_rgba(&self) -> Option<Vec<u8>> {
            self.handler.last_frame_rgba()
        }

        fn interactive_rects(&self) -> Vec<geometry_core::Rect> {
            self.handler.interactive_rects()
        }
    }
}
