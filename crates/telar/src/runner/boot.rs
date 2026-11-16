//! What every entry point does before a surface draws: install the app's directories and preference store, bring back what it remembers, and build the handler that drives the surface.

use std::sync::Arc;

use platform_core::{UriSchemes, Window, WindowConfig};
use renderer_core::DebugSettings;
use services_core::AppPathsProvider;
use telar_project::protocol::RendererBackend;
use ui_tree::DevOverlay;

use crate::app_config::AppConfig;
use crate::app_runtime::AppRuntime;
use crate::config;

use super::font_config::FontSetup;
use super::handler::{AppHandler, build_app_handler};
use super::host::SurfaceRenderer;
use super::state::WindowTitle;

/// When the choices Telar keeps in the preference store are brought back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StoredChoices {
    Now,
    /// Once the prerendered page the surface takes over has been restored, since its first tree is built from the choices the page was written with.
    AfterHydration,
}

/// Installs `app_name`'s directories and preference store, which the first boot in a process keeps, and brings back the choices kept there on this thread.
pub(super) fn install_app(
    app_name: &str,
    paths: Arc<dyn AppPathsProvider>,
    choices: StoredChoices,
) {
    services_core::app_paths::install(app_name, paths);
    crate::user_preferences::install_store(app_name);
    if choices == StoredChoices::Now {
        crate::user_preferences::follow_stored_choices();
    }
}

/// The window an [`AppConfig`] opens, the faces and family its surface sets text in, and the URI schemes its links go to.
pub(super) fn split_config(config: AppConfig) -> (WindowConfig, FontSetup, UriSchemes) {
    let AppConfig {
        window,
        fonts: faces,
        font_family,
        uri_schemes,
    } = config;
    (
        window,
        FontSetup {
            faces,
            family: font_family,
        },
        uri_schemes,
    )
}

/// An app installed for a surface, with the renderer backend it reopens with and the developer switches it runs under: what every entry point builds its handler from.
pub(super) struct SurfaceBoot {
    app_name: String,
    paths: Arc<dyn AppPathsProvider>,
    backend: RendererBackend,
    debug: DebugSettings,
    fonts: FontSetup,
    uri_schemes: UriSchemes,
}

impl SurfaceBoot {
    /// Installs the app (see [`install_app`]) and reads the renderer backend kept in its preference store; the surface runs under `debug`.
    pub(super) fn new(
        app_name: &str,
        paths: Arc<dyn AppPathsProvider>,
        fonts: FontSetup,
        uri_schemes: UriSchemes,
        choices: StoredChoices,
        debug: DebugSettings,
    ) -> Self {
        install_app(app_name, paths.clone(), choices);
        let backend =
            crate::user_preferences::stored_backend().unwrap_or(config::COMPILE_TIME_BACKEND);
        Self {
            app_name: app_name.to_owned(),
            paths,
            backend,
            debug,
            fonts,
            uri_schemes,
        }
    }

    /// The handler that drives `app` on a surface opened under `title`.
    pub(super) fn handler<W, D>(
        self,
        app: Box<dyn AppRuntime>,
        title: &str,
        renderer: SurfaceRenderer<W>,
    ) -> AppHandler<W, D>
    where
        W: Window + Clone + 'static,
        D: DevOverlay,
    {
        let mut handler = build_app_handler::<W, D>(
            app,
            self.paths,
            self.fonts,
            self.backend,
            self.debug,
            self.app_name,
            renderer,
        );
        handler.title = Some(WindowTitle::opened_as(title));
        handler.uri_schemes = self.uri_schemes;
        handler
    }
}

#[cfg(test)]
#[path = "boot.test.rs"]
mod tests;
