use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use reactive_core::{FlushNotifyHandle, set_flush_notify};
use renderer_core::{RenderBackend, RendererError};
use renderer_hardware::HardwareRenderer;
use renderer_software::SoftwareRenderer;
use ui_core::ComponentTree;

use crate::app::App;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::window_signals::WindowSignals;

struct AppHandler {
    app: Box<dyn App>,
    tree: Option<ComponentTree>,
    renderer: Option<Box<dyn RenderBackend>>,
    backend: RendererBackend,
    prefs: UserPrefs,
    pending_restart: bool,
    _flush_notify: Option<FlushNotifyHandle>,
    window_signals: Option<WindowSignals>,
    app_name: String,
}

impl EventHandler<WinitWindow> for AppHandler {
    fn on_resume(&mut self, window: &WinitWindow) -> bool {
        match create_renderer(self.backend, window) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                tracing::error!("Failed to initialize renderer: {e}");
                return false;
            }
        }
        self.window_signals = Some(WindowSignals::new(
            window.width() as f32,
            window.height() as f32,
        ));
        self.tree = Some(ComponentTree::new(self.app.root()));

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        window.request_redraw();
        true
    }

    fn on_event(&mut self, event: Event, window: &WinitWindow) {
        if let Event::WindowResized { width, height } = &event {
            if let Some(ref signals) = self.window_signals {
                signals.update(*width as f32, *height as f32);
            }
        }
        if let Some(tree) = &mut self.tree {
            if tree.on_event(&event).is_handled() {
                window.request_redraw();
            }
        }
    }

    fn on_redraw(&mut self, window: &WinitWindow) {
        let mut redraw_requested = false;
        {
            let mut ctx = crate::app_context::AppCtx {
                app_name: &self.app_name,
                prefs: &mut self.prefs,
                pending_restart: &mut self.pending_restart,
                redraw_requested: &mut redraw_requested,
                window_signals: self.window_signals.as_ref(),
            };
            self.app.on_frame(&mut ctx);
        }
        if redraw_requested {
            window.request_redraw();
        }

        if self.pending_restart {
            self.pending_restart = false;
            self.backend = self
                .prefs
                .backend
                .unwrap_or_else(config::compile_time_backend);
            match create_renderer(self.backend, window) {
                Ok(renderer) => self.renderer = Some(renderer),
                Err(e) => tracing::error!("Failed to switch renderer: {e}"),
            }
        }

        let Some(renderer) = &mut self.renderer else {
            return;
        };
        if let Err(e) = renderer.begin_frame(window.width(), window.height()) {
            tracing::error!("begin_frame failed: {e}");
            return;
        }
        let clear = self.app.clear_color();
        let commands = self.tree.as_ref().map(|t| t.commands()).unwrap_or_default();
        if let Err(e) = renderer.as_mut().submit(commands) {
            tracing::error!("submit failed: {e}");
            return;
        }
        if let Err(e) = renderer.as_mut().end_frame(clear) {
            tracing::error!("end_frame failed: {e}");
        }
    }

    fn on_suspend(&mut self) {}
}

fn create_renderer(
    backend: RendererBackend,
    window: &WinitWindow,
) -> Result<Box<dyn RenderBackend>, RendererError> {
    match backend {
        RendererBackend::Auto => match HardwareRenderer::new(window.clone()) {
            Ok(renderer) => {
                tracing::info!("Using hardware renderer");
                Ok(Box::new(renderer))
            }
            Err(e) => {
                tracing::warn!(
                    "Hardware renderer unavailable ({e}), falling back to software renderer"
                );
                SoftwareRenderer::new(window.clone(), window.clone())
                    .map(|r| Box::new(r) as Box<dyn RenderBackend>)
            }
        },
        RendererBackend::Hardware => HardwareRenderer::new(window.clone()).map(|r| {
            tracing::info!("Using hardware renderer");
            Box::new(r) as Box<dyn RenderBackend>
        }),
        RendererBackend::Software => {
            tracing::info!("Using software renderer");
            SoftwareRenderer::new(window.clone(), window.clone())
                .map(|r| Box::new(r) as Box<dyn RenderBackend>)
        }
    }
}

pub fn run_app_with_name<A: App>(config: WindowConfig, app: A, app_name: &str) {
    let prefs = UserPrefs::load(app_name);
    let backend = prefs.backend.unwrap_or_else(config::compile_time_backend);

    let platform = match WinitPlatform::try_new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to create event loop: {e}");
            return;
        }
    };
    if let Err(e) = platform.run(
        config,
        AppHandler {
            app: Box::new(app),
            tree: None,
            renderer: None,
            backend,
            prefs,
            pending_restart: false,
            _flush_notify: None,
            window_signals: None,
            app_name: app_name.to_owned(),
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
    }
}
