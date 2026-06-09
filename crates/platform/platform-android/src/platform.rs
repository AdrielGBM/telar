use android_activity::AndroidApp;
use platform_core::{
    Event, EventHandler, Platform, PlatformError, PointerButton, PointerSource, ScrollDelta,
    Window, WindowConfig,
};

// ANativeWindow_setFrameRate is API 30+ and may live in libnativewindow.so on some OEM devices rather than libandroid.so, so resolve it at runtime to avoid a hard dlopen failure on devices where the NDK stub does not match the runtime library.
#[cfg(target_os = "android")]
unsafe fn try_set_frame_rate(window: *mut std::ffi::c_void, fps: f32) {
    unsafe extern "C" {
        fn dlsym(
            handle: *mut core::ffi::c_void,
            symbol: *const core::ffi::c_char,
        ) -> *mut core::ffi::c_void;
    }
    let sym = unsafe {
        dlsym(
            core::ptr::null_mut(),
            b"ANativeWindow_setFrameRate\0".as_ptr() as _,
        )
    };
    if sym.is_null() {
        return;
    }
    let f: unsafe extern "C" fn(*mut core::ffi::c_void, f32, i8) -> i32 =
        unsafe { core::mem::transmute(sym) };
    unsafe { f(window, fps, 0) };
}

use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, MouseButton as WinitMouseButton, MouseScrollDelta, StartCause, Touch, TouchPhase,
    WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::window::{WindowAttributes, WindowId};

use crate::window::AndroidWindow;

pub struct AndroidPlatform {
    event_loop: EventLoop<()>,
}

impl AndroidPlatform {
    pub fn new(app: AndroidApp) -> Result<Self, PlatformError> {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(log::LevelFilter::Debug),
        );
        let event_loop = EventLoop::builder()
            .with_android_app(app)
            .build()
            .map_err(|e| PlatformError(e.to_string()))?;
        Ok(Self { event_loop })
    }
}

struct AndroidRunner<H: EventHandler<AndroidWindow>> {
    handler: H,
    window: Option<AndroidWindow>,
    config: WindowConfig,
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
    cursor_pos: (f64, f64),
    // Last position of an active touch finger, used to emit Scrolled deltas from drag gestures.
    last_touch_pos: Option<(f64, f64, u64)>,
    // True only on WaitUntil timer expiry; gates keepalive request_redraw() so it doesn't fire on every event queue drain.
    timer_fired: bool,
}

impl<H: EventHandler<AndroidWindow>> ApplicationHandler for AndroidRunner<H> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        self.timer_fired = matches!(cause, StartCause::ResumeTimeReached { .. });
        self.handler.new_events();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(d) = self.handler.about_to_wait() {
            if self.timer_fired {
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
        let attrs = WindowAttributes::default()
            .with_title(self.config.title.as_str())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ));
        use std::sync::Arc;
        match event_loop.create_window(attrs) {
            Ok(w) => {
                // Read before move into Arc; ScaleFactorChanged may not fire on first resume.
                self.scale_factor = w.scale_factor();
                #[cfg(target_os = "android")]
                {
                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    if let Ok(handle) = w.window_handle() {
                        if let RawWindowHandle::AndroidNdk(android_handle) = handle.as_raw() {
                            unsafe {
                                try_set_frame_rate(android_handle.a_native_window.as_ptr(), 60.0);
                            }
                        }
                    }
                }
                let window = AndroidWindow(Arc::new(w));
                if !self.handler.on_resume(&window) {
                    event_loop.exit();
                    return;
                }
                window.request_redraw();
                self.window = Some(window);
            }
            Err(e) => eprintln!("[rsx] failed to create window: {e}"),
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.handler.on_suspend();
        // On Android the native window is destroyed on suspend; drop our reference so it can be recreated on resume.
        self.window = None;
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
            WindowEvent::Touch(Touch {
                phase,
                location,
                id,
                ..
            }) => {
                let x = location.x / self.scale_factor;
                let y = location.y / self.scale_factor;
                let source = PointerSource::Touch { id };
                match phase {
                    TouchPhase::Started => {
                        self.last_touch_pos = Some((x, y, id));
                        self.handler.on_event(
                            Event::PointerPressed {
                                x,
                                y,
                                button: PointerButton::Primary,
                                source,
                            },
                            window,
                        );
                    }
                    TouchPhase::Moved => {
                        if let Some((lx, ly, lid)) = self.last_touch_pos {
                            if lid == id {
                                let dx = x - lx;
                                let dy = y - ly;
                                self.handler.on_event(
                                    Event::Scrolled {
                                        delta: platform_core::ScrollDelta::Pixels {
                                            x: dx as f32,
                                            y: dy as f32,
                                        },
                                    },
                                    window,
                                );
                            }
                        }
                        self.last_touch_pos = Some((x, y, id));
                        self.handler
                            .on_event(Event::PointerMoved { x, y, source }, window);
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.last_touch_pos = None;
                        self.handler.on_event(
                            Event::PointerReleased {
                                x,
                                y,
                                button: PointerButton::Primary,
                                source,
                            },
                            window,
                        );
                    }
                }
            }
            WindowEvent::Focused(gained) => {
                self.handler
                    .on_event(Event::FocusChanged { gained }, window);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.handler
                    .on_event(Event::ScaleFactorChanged { scale_factor }, window);
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
                let modifiers = self.modifiers;
                let ev = match event.state {
                    ElementState::Pressed => platform_core::Event::KeyPressed { key, modifiers },
                    ElementState::Released => platform_core::Event::KeyReleased { key, modifiers },
                };
                self.handler.on_event(ev, window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let lx = position.x / self.scale_factor;
                let ly = position.y / self.scale_factor;
                self.cursor_pos = (lx, ly);
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
            WindowEvent::CursorEntered { .. } => {
                self.handler.on_event(Event::CursorEntered, window);
            }
            WindowEvent::CursorLeft { .. } => {
                self.handler.on_event(Event::CursorLeft, window);
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
            _ => {}
        }
    }
}

impl Platform for AndroidPlatform {
    type Window = AndroidWindow;

    fn run<H: EventHandler<Self::Window>>(
        self,
        config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError> {
        let mut runner = AndroidRunner {
            handler,
            window: None,
            config,
            scale_factor: 1.0,
            modifiers: platform_core::ModifiersState::default(),
            cursor_pos: (0.0, 0.0),
            last_touch_pos: None,
            timer_fired: false,
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
