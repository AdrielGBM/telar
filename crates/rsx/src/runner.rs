use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use renderer_core::RenderBackend;
use renderer_hardware::HardwareRenderer;
use renderer_software::SoftwareRenderer;

use crate::app::{App, Frame};
use crate::config::{self, RendererBackend};
use crate::context::AppContext;
use crate::prefs::UserPrefs;

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
        self.renderer = Some(create_renderer(self.backend, window));
        let mut ctx = AppContext {
            app_name: &self.app_name,
            prefs: &mut self.prefs,
            pending_restart: &mut self.pending_restart,
        };
        self.app.on_resume(&mut ctx);
    }

    fn on_event(&mut self, event: Event, _window: &WinitWindow) {
        let mut ctx = AppContext {
            app_name: &self.app_name,
            prefs: &mut self.prefs,
            pending_restart: &mut self.pending_restart,
        };
        self.app.on_event(event, &mut ctx);
    }

    fn on_redraw(&mut self, window: &WinitWindow) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.begin_frame(window.width(), window.height());
        let app = &mut self.app;
        let mut frame = Frame {
            renderer: renderer.as_mut(),
        };
        let mut ctx = AppContext {
            app_name: &self.app_name,
            prefs: &mut self.prefs,
            pending_restart: &mut self.pending_restart,
        };
        app.on_redraw(&mut frame, &mut ctx);
        frame.renderer.end_frame();
    }

    fn on_suspend(&mut self) {
        let mut ctx = AppContext {
            app_name: &self.app_name,
            prefs: &mut self.prefs,
            pending_restart: &mut self.pending_restart,
        };
        self.app.on_suspend(&mut ctx);
    }
}

fn create_renderer(backend: RendererBackend, window: &WinitWindow) -> Box<dyn RenderBackend> {
    match backend {
        RendererBackend::Auto => match HardwareRenderer::new(window.clone()) {
            Ok(renderer) => {
                eprintln!("[rsx] Using GPU renderer");
                Box::new(renderer)
            }
            Err(e) => {
                eprintln!("[rsx] GPU unavailable ({e}), falling back to CPU renderer");
                Box::new(SoftwareRenderer::new(window.clone(), window.clone()))
            }
        },
        RendererBackend::Gpu => HardwareRenderer::new(window.clone())
            .map(|r| {
                eprintln!("[rsx] Using GPU renderer");
                Box::new(r) as Box<dyn RenderBackend>
            })
            .expect("[rsx] GPU renderer was requested but failed to initialize"),
        RendererBackend::Cpu => {
            eprintln!("[rsx] Using CPU renderer");
            Box::new(SoftwareRenderer::new(window.clone(), window.clone()))
        }
    }
}

fn resolve_backend(config_backend: RendererBackend, prefs: &UserPrefs) -> RendererBackend {
    prefs.renderer.backend.unwrap_or(config_backend)
}

pub fn run_with_name<A: App>(config: WindowConfig, app: A, app_name: &str) {
    let prefs = UserPrefs::load(app_name);
    let backend = resolve_backend(config::compile_time_backend(), &prefs);

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
