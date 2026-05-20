use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use reactive_core::{FlushNotifyHandle, set_flush_notify};
use renderer_core::{RenderBackend, RendererError};
use renderer_hardware::HardwareRenderer;
use renderer_software::SoftwareRenderer;

use crate::app::{App, Frame};
use crate::app_context::AppCtx;
use crate::config::{self, RendererBackend};
use crate::prefs::UserPrefs;
use crate::reactive_app::ReactiveApp;
use crate::reactive_runner::ReactiveAdapter;
use crate::window_signals::WindowSignals;

macro_rules! make_ctx {
    ($self:expr) => {
        AppCtx {
            app_name: &$self.app_name,
            prefs: &mut $self.prefs,
            pending_restart: &mut $self.pending_restart,
            redraw_requested: &mut $self.redraw_requested,
            window_signals: $self.window_signals.as_ref(),
        }
    };
}

struct AppHandler<A: App> {
    app: A,
    renderer: Option<Box<dyn RenderBackend>>,
    backend: RendererBackend,
    app_name: String,
    prefs: UserPrefs,
    pending_restart: bool,
    redraw_requested: bool,
    _flush_notify: Option<FlushNotifyHandle>,
    window_signals: Option<WindowSignals>,
}

impl<A: App> EventHandler<WinitWindow> for AppHandler<A> {
    fn on_resume(&mut self, window: &WinitWindow) -> bool {
        match create_renderer(self.backend, window) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => {
                tracing::error!("Failed to initialize renderer: {e}");
                return false;
            }
        }
        let initial_width = window.width() as f32;
        let initial_height = window.height() as f32;
        self.window_signals = Some(WindowSignals::new(initial_width, initial_height));
        self.redraw_requested = false;
        if let Err(e) = self.app.on_resume(&mut make_ctx!(self)) {
            tracing::error!("App::on_resume failed: {e}");
            return false;
        }

        let w = window.clone();
        self._flush_notify = Some(set_flush_notify(move || w.request_redraw()));
        window.request_redraw();
        true
    }

    fn on_event(&mut self, event: Event, window: &WinitWindow) {
        self.redraw_requested = false;
        if let Event::WindowResized { width, height } = &event {
            if let Some(ref signals) = self.window_signals {
                signals.update(*width as f32, *height as f32);
            }
        }
        self.app.on_event(event, &mut make_ctx!(self));
        if self.redraw_requested {
            window.request_redraw();
        }
    }

    fn on_redraw(&mut self, window: &WinitWindow) {
        if self.pending_restart {
            self.pending_restart = false;
            self.backend = self
                .prefs
                .renderer
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
        self.redraw_requested = false;
        let app = &mut self.app;
        let mut frame = Frame::new();
        app.on_redraw(&mut frame, &mut make_ctx!(self));
        let commands = std::mem::take(&mut frame.commands);
        let clear = frame.clear_color;
        if let Err(e) = renderer.as_mut().submit(commands) {
            tracing::error!("submit failed: {e}");
            return;
        }
        if let Err(e) = renderer.as_mut().end_frame(clear) {
            tracing::error!("end_frame failed: {e}");
        }
        if self.redraw_requested {
            window.request_redraw();
        }
    }

    fn on_suspend(&mut self) {
        self.redraw_requested = false;
        self.app.on_suspend(&mut make_ctx!(self));
    }
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

pub fn run_reactive_with_name<R: ReactiveApp>(config: WindowConfig, app: R, app_name: &str) {
    run_with_name(config, ReactiveAdapter::new(app), app_name);
}

pub(crate) fn run_with_name<A: App>(config: WindowConfig, app: A, app_name: &str) {
    let prefs = UserPrefs::load(app_name);
    let backend = prefs
        .renderer
        .backend
        .unwrap_or_else(config::compile_time_backend);

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
            app,
            renderer: None,
            backend,
            app_name: app_name.to_string(),
            prefs,
            pending_restart: false,
            redraw_requested: false,
            _flush_notify: None,
            window_signals: None,
        },
    ) {
        tracing::error!("Event loop exited with error: {e}");
    }
}
