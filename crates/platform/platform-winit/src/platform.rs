use platform_core::{
    Event, EventHandler, Platform, PlatformError, PointerButton, PointerSource, Window,
    WindowConfig,
};
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{WindowAttributes, WindowId};

use crate::window::WinitWindow;

pub struct WinitPlatform {
    event_loop: EventLoop<()>,
}

impl WinitPlatform {
    pub fn try_new() -> Result<Self, winit::error::EventLoopError> {
        Ok(Self {
            event_loop: EventLoop::new()?,
        })
    }

    pub fn new() -> Self {
        Self::try_new().expect("failed to create winit event loop")
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
    modifiers: platform_core::ModifiersState,
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
        match event_loop.create_window(attrs) {
            Ok(w) => {
                let window = WinitWindow(Arc::new(w));
                self.handler.on_resume(&window);
                window.request_redraw();
                self.window = Some(window);
            }
            Err(e) => eprintln!("[rsx] failed to create window: {e}"),
        }
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
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = platform_core::ModifiersState {
                    shift: mods.state().shift_key(),
                    ctrl: mods.state().control_key(),
                    alt: mods.state().alt_key(),
                    meta: mods.state().super_key(),
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = match &event.logical_key {
                    WinitKey::Character(c) => {
                        if let Some(ch) = c.as_str().chars().next() {
                            platform_core::Key::Char(ch)
                        } else {
                            return;
                        }
                    }
                    WinitKey::Named(named) => {
                        let nk = match named {
                            WinitNamedKey::Enter => platform_core::NamedKey::Enter,
                            WinitNamedKey::Backspace => platform_core::NamedKey::Backspace,
                            WinitNamedKey::Escape => platform_core::NamedKey::Escape,
                            WinitNamedKey::Tab => platform_core::NamedKey::Tab,
                            WinitNamedKey::Delete => platform_core::NamedKey::Delete,
                            WinitNamedKey::Home => platform_core::NamedKey::Home,
                            WinitNamedKey::End => platform_core::NamedKey::End,
                            WinitNamedKey::PageUp => platform_core::NamedKey::PageUp,
                            WinitNamedKey::PageDown => platform_core::NamedKey::PageDown,
                            WinitNamedKey::ArrowUp => platform_core::NamedKey::ArrowUp,
                            WinitNamedKey::ArrowDown => platform_core::NamedKey::ArrowDown,
                            WinitNamedKey::ArrowLeft => platform_core::NamedKey::ArrowLeft,
                            WinitNamedKey::ArrowRight => platform_core::NamedKey::ArrowRight,
                            WinitNamedKey::F1 => platform_core::NamedKey::F1,
                            WinitNamedKey::F2 => platform_core::NamedKey::F2,
                            WinitNamedKey::F3 => platform_core::NamedKey::F3,
                            WinitNamedKey::F4 => platform_core::NamedKey::F4,
                            WinitNamedKey::F5 => platform_core::NamedKey::F5,
                            WinitNamedKey::F6 => platform_core::NamedKey::F6,
                            WinitNamedKey::F7 => platform_core::NamedKey::F7,
                            WinitNamedKey::F8 => platform_core::NamedKey::F8,
                            WinitNamedKey::F9 => platform_core::NamedKey::F9,
                            WinitNamedKey::F10 => platform_core::NamedKey::F10,
                            WinitNamedKey::F11 => platform_core::NamedKey::F11,
                            WinitNamedKey::F12 => platform_core::NamedKey::F12,
                            WinitNamedKey::Space => platform_core::NamedKey::Space,
                            WinitNamedKey::Insert => platform_core::NamedKey::Insert,
                            WinitNamedKey::CapsLock => platform_core::NamedKey::CapsLock,
                            _ => return,
                        };
                        platform_core::Key::Named(nk)
                    }
                    _ => return,
                };
                let modifiers = self.modifiers.clone();
                let ev = match event.state {
                    ElementState::Pressed => platform_core::Event::KeyPressed { key, modifiers },
                    ElementState::Released => platform_core::Event::KeyReleased { key, modifiers },
                };
                self.handler.on_event(ev, window);
            }
            _ => {}
        }
    }
}

impl Platform for WinitPlatform {
    type Window = WinitWindow;

    fn run<H: EventHandler<Self::Window>>(
        self,
        config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError> {
        let mut runner = WinitRunner {
            handler,
            window: None,
            config,
            cursor_pos: (0.0, 0.0),
            modifiers: platform_core::ModifiersState::default(),
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
