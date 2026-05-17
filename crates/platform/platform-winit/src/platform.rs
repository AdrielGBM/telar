use platform_core::{
    Event, EventHandler, Platform, PointerButton, PointerSource, Window, WindowConfig,
};
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
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
                    Event::PointerMoved {
                        x: position.x,
                        y: position.y,
                        source: PointerSource::Mouse,
                    },
                    window,
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WinitMouseButton::Left => PointerButton::Primary,
                    WinitMouseButton::Right => PointerButton::Secondary,
                    WinitMouseButton::Middle => PointerButton::Auxiliary,
                    _ => return,
                };
                let (x, y) = self.cursor_pos;
                let ev = match state {
                    ElementState::Pressed => Event::PointerPressed {
                        x,
                        y,
                        button: btn,
                        source: PointerSource::Mouse,
                    },
                    ElementState::Released => Event::PointerReleased {
                        x,
                        y,
                        button: btn,
                        source: PointerSource::Mouse,
                    },
                };
                self.handler.on_event(ev, window);
            }
            WindowEvent::Touch(Touch {
                phase,
                location,
                id,
                ..
            }) => {
                let x = location.x;
                let y = location.y;
                let source = PointerSource::Touch { id };
                let ev = match phase {
                    TouchPhase::Started => Event::PointerPressed {
                        x,
                        y,
                        button: PointerButton::Primary,
                        source,
                    },
                    TouchPhase::Moved => Event::PointerMoved { x, y, source },
                    TouchPhase::Ended | TouchPhase::Cancelled => Event::PointerReleased {
                        x,
                        y,
                        button: PointerButton::Primary,
                        source,
                    },
                };
                self.handler.on_event(ev, window);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x as f64 * 20.0, y as f64 * 20.0),
                    MouseScrollDelta::PixelDelta(pos) => (pos.x, pos.y),
                };
                self.handler
                    .on_event(Event::Scrolled { delta_x, delta_y }, window);
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
