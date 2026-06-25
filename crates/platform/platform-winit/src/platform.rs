use platform_core::{
    Event, EventHandler, FullscreenMode, Platform, PlatformError, PointerButton, PointerSource,
    ScrollDelta, Window, WindowConfig, WindowPosition,
};
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, StartCause, Touch, TouchPhase,
    WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Fullscreen, WindowAttributes, WindowId, WindowLevel};

use crate::window::WinitWindow;

pub struct WinitPlatform {
    event_loop: EventLoop<()>,
}

impl WinitPlatform {
    pub fn try_new() -> Result<Self, PlatformError> {
        Ok(Self {
            event_loop: EventLoop::new().map_err(|e| PlatformError(e.to_string()))?,
        })
    }
}

struct WinitRunner<H: EventHandler<WinitWindow>> {
    handler: H,
    window: Option<WinitWindow>,
    config: WindowConfig,
    cursor_position: (f64, f64),
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
    // True only on WaitUntil timer expiry; gates keepalive request_redraw() so it doesn't fire on every event queue drain.
    timer_has_fired: bool,
}

impl<H: EventHandler<WinitWindow>> ApplicationHandler for WinitRunner<H> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        self.timer_has_fired = matches!(cause, StartCause::ResumeTimeReached { .. });
        self.handler.new_events();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(d) = self.handler.about_to_wait() {
            // Only request_redraw() on timer expiry, not every drain; reactive changes call it themselves via flush_notify.
            if self.timer_has_fired {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + d));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut attributes = WindowAttributes::default()
            .with_title(self.config.title.as_str())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ))
            .with_resizable(self.config.is_resizable)
            .with_decorations(self.config.has_decorations)
            .with_transparent(self.config.is_transparent);

        if let Some((w, h)) = self.config.min_size {
            attributes = attributes.with_min_inner_size(winit::dpi::LogicalSize::new(w, h));
        }
        if let Some((w, h)) = self.config.max_size {
            attributes = attributes.with_max_inner_size(winit::dpi::LogicalSize::new(w, h));
        }
        match self.config.fullscreen {
            FullscreenMode::Disabled => {}
            // Exclusive requires a concrete video mode; fall back to borderless for now.
            FullscreenMode::Borderless | FullscreenMode::Exclusive => {
                attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        }
        if let WindowPosition::At(x, y) = self.config.position {
            attributes = attributes.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        if self.config.is_always_on_top {
            attributes = attributes.with_window_level(WindowLevel::AlwaysOnTop);
        }

        use std::sync::Arc;
        match event_loop.create_window(attributes) {
            Ok(w) => {
                let window = WinitWindow(Arc::new(w));
                if !self.handler.on_resume(&window) {
                    event_loop.exit();
                    return;
                }
                window.request_redraw();
                self.window = Some(window);
            }
            Err(e) => tracing::error!(error = %e, "failed to create window"),
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
                        width: (size.width as f64 / self.scale_factor).round() as u32,
                        height: (size.height as f64 / self.scale_factor).round() as u32,
                    },
                    window,
                );
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.handler.on_redraw(window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let lx = position.x / self.scale_factor;
                let ly = position.y / self.scale_factor;
                self.cursor_position = (lx, ly);
                self.handler.on_event(
                    Event::PointerMoved {
                        x: lx,
                        y: ly,
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
                let (x, y) = self.cursor_position;
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
                let x = location.x / self.scale_factor;
                let y = location.y / self.scale_factor;
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
            WindowEvent::Focused(is_focused) => {
                self.handler
                    .on_event(Event::FocusChanged { is_focused }, window);
            }
            WindowEvent::CursorEntered { .. } => {
                self.handler.on_event(Event::CursorEntered, window);
            }
            WindowEvent::CursorLeft { .. } => {
                self.handler.on_event(Event::CursorLeft, window);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.handler
                    .on_event(Event::ScaleFactorChanged { scale_factor }, window);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines { x, y },
                    MouseScrollDelta::PixelDelta(pos) => ScrollDelta::Pixels {
                        x: (pos.x / self.scale_factor) as f32,
                        y: (pos.y / self.scale_factor) as f32,
                    },
                };
                self.handler.on_event(
                    Event::Scrolled {
                        delta: scroll_delta,
                    },
                    window,
                );
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = platform_core::ModifiersState {
                    is_shift: mods.state().shift_key(),
                    is_ctrl: mods.state().control_key(),
                    is_alt: mods.state().alt_key(),
                    is_meta: mods.state().super_key(),
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
                let modifiers = self.modifiers;
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
            cursor_position: (0.0, 0.0),
            scale_factor: 1.0,
            modifiers: platform_core::ModifiersState::default(),
            timer_has_fired: false,
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
