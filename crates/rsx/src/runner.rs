use platform_core::{Event, EventHandler, Platform, Window, WindowConfig};
use platform_winit::{WinitPlatform, WinitWindow};
use renderer_core::RenderBackend;
use renderer_software::SoftwareRenderer;

use crate::app::{App, Frame};

struct AppHandler<A: App> {
    app: A,
    renderer: Option<SoftwareRenderer<WinitWindow, WinitWindow>>,
}

impl<A: App> EventHandler<WinitWindow> for AppHandler<A> {
    fn on_resume(&mut self, window: &WinitWindow) {
        self.renderer = Some(SoftwareRenderer::new(window.clone(), window.clone()));
        self.app.on_resume();
    }

    fn on_event(&mut self, event: Event, _window: &WinitWindow) {
        self.app.on_event(event);
    }

    fn on_redraw(&mut self, window: &WinitWindow) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        renderer.begin_frame(window.width(), window.height());
        let app = &mut self.app;
        let mut frame = Frame { renderer };
        app.on_redraw(&mut frame);
        frame.renderer.end_frame();
    }

    fn on_suspend(&mut self) {
        self.app.on_suspend();
    }
}

pub fn run<A: App>(config: WindowConfig, app: A) {
    WinitPlatform::new().run(
        config,
        AppHandler {
            app,
            renderer: None,
        },
    );
}
