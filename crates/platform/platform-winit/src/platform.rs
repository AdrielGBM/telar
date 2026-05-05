use platform_core::{Event, EventHandler, MouseButton, Platform, Window, WindowConfig};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{WindowAttributes, WindowId};

use crate::window::WinitWindow;

pub struct WinitPlatform {
    event_loop: EventLoop<()>,
}

impl WinitPlatform {
    pub fn new() -> Self {
        Self {
            event_loop: EventLoop::new().unwrap(),
        }
    }
}

impl Default for WinitPlatform {
    fn default() -> Self {
        Self::new()
    }
}

struct WinitRunner<H: EventHandler<WinitWindow>> {
    handler: H,
    window: Option<WinitWindow>,
    config: WindowConfig,
    cursor_pos: (f64, f64),
}

impl<H: EventHandler<WinitWindow>> ApplicationHandler for WinitRunner<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title(self.config.title.as_str())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ));
        use std::sync::Arc;
        let window = WinitWindow(Arc::new(event_loop.create_window(attrs).unwrap()));
        self.handler.on_resume(&window);
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else { return };
        match event {
            WindowEvent::CloseRequested => {
                self.handler.on_event(Event::WindowCloseRequested, window);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.handler.on_event(
                    Event::WindowResized {
                        width: size.width,
                        height: size.height,
                    },
                    window,
                );
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.handler.on_redraw(window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x, position.y);
                self.handler.on_event(
                    Event::MouseMoved {
                        x: position.x,
                        y: position.y,
                    },
                    window,
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Right => MouseButton::Right,
                    WinitMouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let (x, y) = self.cursor_pos;
                let ev = match state {
                    ElementState::Pressed => Event::MousePressed { x, y, button: btn },
                    ElementState::Released => Event::MouseReleased { x, y, button: btn },
                };
                self.handler.on_event(ev, window);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(text) = event.logical_key.to_text() {
                    let ev = match event.state {
                        ElementState::Pressed => Event::KeyPressed {
                            key: text.to_string(),
                        },
                        ElementState::Released => Event::KeyReleased {
                            key: text.to_string(),
                        },
                    };
                    self.handler.on_event(ev, window);
                }
            }
            _ => {}
        }
    }
}

impl Platform for WinitPlatform {
    type Window = WinitWindow;

    fn run<H: EventHandler<Self::Window>>(self, config: WindowConfig, handler: H) {
        let mut runner = WinitRunner {
            handler,
            window: None,
            config,
            cursor_pos: (0.0, 0.0),
        };
        self.event_loop.run_app(&mut runner).unwrap();
    }
}
