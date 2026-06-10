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

// AChoreographer is API 24+ (Android 7.0). Resolved at runtime via dlsym to avoid hard-linking failures on older NDK stubs or OEM variants.
#[cfg(target_os = "android")]
mod choreographer {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Opaque handle — AChoreographer is not meant to be constructed, only passed through.
    #[repr(C)]
    pub struct AChoreographer {
        _opaque: [u8; 0],
    }

    pub type FrameCallbackFn =
        unsafe extern "C" fn(frame_time_ns: i64, data: *mut core::ffi::c_void);

    // Resolve AChoreographer_getInstance at runtime; returns null on pre-API-24 devices.
    unsafe fn get_instance_fn() -> Option<unsafe extern "C" fn() -> *mut AChoreographer> {
        unsafe extern "C" {
            fn dlsym(
                handle: *mut core::ffi::c_void,
                symbol: *const core::ffi::c_char,
            ) -> *mut core::ffi::c_void;
        }
        let sym = unsafe {
            dlsym(
                core::ptr::null_mut(),
                b"AChoreographer_getInstance\0".as_ptr() as _,
            )
        };
        if sym.is_null() {
            None
        } else {
            Some(unsafe { core::mem::transmute(sym) })
        }
    }

    // Resolve AChoreographer_postFrameCallback at runtime.
    unsafe fn post_callback_fn()
    -> Option<unsafe extern "C" fn(*mut AChoreographer, FrameCallbackFn, *mut core::ffi::c_void)>
    {
        unsafe extern "C" {
            fn dlsym(
                handle: *mut core::ffi::c_void,
                symbol: *const core::ffi::c_char,
            ) -> *mut core::ffi::c_void;
        }
        let sym = unsafe {
            dlsym(
                core::ptr::null_mut(),
                b"AChoreographer_postFrameCallback\0".as_ptr() as _,
            )
        };
        if sym.is_null() {
            None
        } else {
            Some(unsafe { core::mem::transmute(sym) })
        }
    }

    // The frame callback: wakes the event loop via the proxy pointer stored in `data`, then clears the pending flag so about_to_wait can re-register on the next animation request.
    pub unsafe extern "C" fn frame_callback(_frame_time_ns: i64, data: *mut core::ffi::c_void) {
        // data points to a VsyncCallbackData on the heap; we only borrow it here.
        let cb_data = unsafe { &*(data as *const VsyncCallbackData) };
        // Clear pending first so about_to_wait sees the frame was delivered.
        cb_data.pending.store(false, Ordering::Release);
        // Wake the winit event loop. Ignore errors — the loop may have already exited.
        let _ = cb_data.proxy.send_event(());
    }

    // Heap-allocated state shared between the runner and the vsync callback. The pointer lives for the full duration of the AndroidRunner.
    pub struct VsyncCallbackData {
        pub pending: Arc<AtomicBool>,
        pub proxy: winit::event_loop::EventLoopProxy<()>,
    }

    pub struct Choreographer {
        // Cached instance pointer; valid for the lifetime of the Looper thread (i.e. the main thread).
        instance: *mut AChoreographer,
        // Stable heap allocation passed as `data` to every postFrameCallback call.
        pub callback_data: Box<VsyncCallbackData>,
    }

    // The instance pointer is obtained on the main thread and only used there, so Send is safe here.
    unsafe impl Send for Choreographer {}

    impl Choreographer {
        // Returns None if AChoreographer is not available on this device/API level.
        pub fn new(
            proxy: winit::event_loop::EventLoopProxy<()>,
            pending: Arc<AtomicBool>,
        ) -> Option<Self> {
            let get_instance = unsafe { get_instance_fn()? };
            let instance = unsafe { get_instance() };
            if instance.is_null() {
                return None;
            }
            Some(Self {
                instance,
                callback_data: Box::new(VsyncCallbackData { pending, proxy }),
            })
        }

        // Post a single vsync callback. No-op if the symbols are unavailable.
        pub fn request_vsync(&self) {
            let post = match unsafe { post_callback_fn() } {
                Some(f) => f,
                None => return,
            };
            // Pass a raw pointer into the stable Box allocation; the Box outlives all callbacks.
            let data_ptr =
                self.callback_data.as_ref() as *const VsyncCallbackData as *mut core::ffi::c_void;
            unsafe { post(self.instance, frame_callback, data_ptr) };
        }
    }
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
    #[cfg(target_os = "android")]
    choreographer: Option<choreographer::Choreographer>,
    #[cfg(target_os = "android")]
    animation_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<H: EventHandler<AndroidWindow>> ApplicationHandler<()> for AndroidRunner<H> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        self.handler.new_events();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(_d) = self.handler.about_to_wait() {
            #[cfg(target_os = "android")]
            {
                // On Android, use Choreographer vsync callbacks instead of WaitUntil wall-clock timers. This aligns frame wakeups to vsync edges, eliminating jank at any refresh rate (60/90/120 Hz).
                let already_pending = self
                    .animation_pending
                    .swap(true, std::sync::atomic::Ordering::AcqRel);
                if !already_pending {
                    if let Some(chore) = &self.choreographer {
                        chore.request_vsync();
                    } else if let Some(window) = &self.window {
                        // Fallback when Choreographer is unavailable (pre-API-24): request an immediate redraw and rely on WaitUntil.
                        window.request_redraw();
                        event_loop.set_control_flow(ControlFlow::WaitUntil(
                            std::time::Instant::now() + _d,
                        ));
                        return;
                    }
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            #[cfg(not(target_os = "android"))]
            {
                event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + _d));
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    // about_to_wait handles frame scheduling; user_event fires when the vsync callback wakes the loop.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
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
        #[cfg(target_os = "android")]
        let animation_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        #[cfg(target_os = "android")]
        let choreographer = choreographer::Choreographer::new(
            self.event_loop.create_proxy(),
            animation_pending.clone(),
        );

        let mut runner = AndroidRunner {
            handler,
            window: None,
            config,
            scale_factor: 1.0,
            modifiers: platform_core::ModifiersState::default(),
            cursor_pos: (0.0, 0.0),
            last_touch_pos: None,
            #[cfg(target_os = "android")]
            choreographer,
            #[cfg(target_os = "android")]
            animation_pending,
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
