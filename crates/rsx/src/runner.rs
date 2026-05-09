use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use renderer_core::{RenderBackend, RendererError};
use renderer_hardware::HardwareRenderer;
use renderer_software::SoftwareRenderer;

use crate::app::{App, Frame};
use crate::config::{self, RendererBackend};
use crate::context::AppContext;
use crate::prefs::UserPrefs;

macro_rules! make_ctx {
    ($self:expr) => {
        AppContext {
            app_name: &$self.app_name,
            prefs: &mut $self.prefs,
            pending_restart: &mut $self.pending_restart,
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
}

impl<A: App> EventHandler<WinitWindow> for AppHandler<A> {
    fn on_resume(&mut self, window: &WinitWindow) {
        match create_renderer(self.backend, window) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(e) => eprintln!("[rsx] Failed to initialize renderer: {e}"),
        }
        self.app.on_resume(&mut make_ctx!(self));
    }

    fn on_event(&mut self, event: Event, _window: &WinitWindow) {
        self.app.on_event(event, &mut make_ctx!(self));
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
                Err(e) => eprintln!("[rsx] Failed to switch renderer: {e}"),
            }
        }

        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.begin_frame(window.width(), window.height());
        let app = &mut self.app;
        let mut frame = Frame {
            renderer: renderer.as_mut(),
            clear_color: None,
        };
        app.on_redraw(&mut frame, &mut make_ctx!(self));
        let clear = frame.clear_color;
        frame.renderer.end_frame(clear);
    }

    fn on_suspend(&mut self) {
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
                eprintln!("[rsx] Using GPU renderer");
                Ok(Box::new(renderer))
            }
            Err(e) => {
                eprintln!(
                    "[rsx] Hardware renderer unavailable ({e}), falling back to software renderer"
                );
                SoftwareRenderer::new(window.clone(), window.clone())
                    .map(|r| Box::new(r) as Box<dyn RenderBackend>)
            }
        },
        RendererBackend::Hardware => HardwareRenderer::new(window.clone()).map(|r| {
            eprintln!("[rsx] Using hardware renderer");
            Box::new(r) as Box<dyn RenderBackend>
        }),
        RendererBackend::Software => {
            eprintln!("[rsx] Using software renderer");
            SoftwareRenderer::new(window.clone(), window.clone())
                .map(|r| Box::new(r) as Box<dyn RenderBackend>)
        }
    }
}

pub fn run_with_name<A: App>(config: WindowConfig, app: A, app_name: &str) {
    let prefs = UserPrefs::load(app_name);
    let backend = prefs
        .renderer
        .backend
        .unwrap_or_else(config::compile_time_backend);

    WinitPlatform::new().run(
        config,
        AppHandler {
            app,
            renderer: None,
            backend,
            app_name: app_name.to_string(),
            prefs,
            pending_restart: false,
        },
    );
}
