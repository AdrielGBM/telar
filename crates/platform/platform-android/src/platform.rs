//! The Android event loop: choreographer-paced frames, and the surface lifecycle an activity puts them through.

use android_activity::AndroidApp;
use platform_core::{Event, EventHandler, Platform, PlatformError, Window, WindowConfig};

// `ANativeWindow_setFrameRate` is API 30+ and may live in libnativewindow.so on some OEM devices, so it is resolved at runtime to avoid a hard dlopen failure where the NDK stub does not match the runtime library.
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
    unsafe fn instance_fn() -> Option<unsafe extern "C" fn() -> *mut AChoreographer> {
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
        let cb_data = unsafe { &*(data as *const VsyncCallbackData) };
        // Clear pending first so about_to_wait sees the frame was delivered.
        cb_data.is_pending.store(false, Ordering::Release);
        // Wake the winit event loop. Ignore errors — the loop may have already exited.
        let _ = cb_data.proxy.send_event(());
    }

    /// Heap-allocated state shared between the runner and the vsync callback. The pointer lives for the full duration of the AndroidRunner.
    pub struct VsyncCallbackData {
        pub is_pending: Arc<AtomicBool>,
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
        /// Returns None if AChoreographer is not available on this device/API level.
        pub fn new(
            proxy: winit::event_loop::EventLoopProxy<()>,
            pending: Arc<AtomicBool>,
        ) -> Option<Self> {
            let get_instance = unsafe { instance_fn()? };
            let instance = unsafe { get_instance() };
            if instance.is_null() {
                return None;
            }
            Some(Self {
                instance,
                callback_data: Box::new(VsyncCallbackData {
                    is_pending: pending,
                    proxy,
                }),
            })
        }

        /// Post a single vsync callback. No-op if the symbols are unavailable.
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
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::window::{WindowAttributes, WindowId};

use platform_winit::{SurfaceIntent, TouchDrag, WinitWindow as AndroidWindow, map_window_event};

/// The Android event loop, driven by `android-activity` and paced by the choreographer.
pub struct AndroidPlatform {
    event_loop: EventLoop<()>,
    // winit's `theme()` is always `None` on Android and the `Window` never sees the activity's configuration, so the OS light/dark preference has to be read from here — the same role the freedesktop portal plays on Linux.
    app: AndroidApp,
}

/// How often the OS light/dark preference is re-read. A theme flip is a human action, so half a second reads as instant, and it keeps a config copy out of every frame.
const THEME_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// The OS light/dark preference, or `None` when the device expresses no opinion.
///
/// Read from the **asset manager**, not from `AndroidApp::config()`. That cached `ConfigurationRef` is only refreshed when android-activity receives NativeActivity's `onConfigurationChanged`, and that callback does not always arrive: on a Redmi/HyperOS device the system applied `night` to the activity — visible in `dumpsys activity activities` as `mLastReportedConfigurations` — while the cached config stayed on the value it had at launch for as long as the process lived. The asset manager tracks the change either way, and it is the same source the glue itself copies from when the callback does fire.
fn prefers_dark(app: &AndroidApp) -> Option<bool> {
    // Through android-activity's own re-export rather than a direct `ndk` dependency, so the types can never be a different version of the ones it uses internally.
    use android_activity::ndk::configuration::{Configuration, UiModeNight};
    match Configuration::from_asset_manager(&app.asset_manager()).ui_mode_night() {
        UiModeNight::Yes => Some(true),
        UiModeNight::No => Some(false),
        _ => None,
    }
}

impl AndroidPlatform {
    pub fn try_new(app: AndroidApp) -> Result<Self, PlatformError> {
        use tracing_subscriber::filter::LevelFilter;
        use tracing_subscriber::prelude::*;

        // Route tracing events (and `log` records bridged from winit/wgpu) to Android logcat under the `rsx` tag. `try_init` is a no-op if a subscriber is already installed.
        let logcat = paranoid_android::layer("telar").with_filter(LevelFilter::DEBUG);
        tracing_subscriber::registry().with(logcat).try_init().ok();

        let event_loop = EventLoop::builder()
            .with_android_app(app.clone())
            .build()
            .map_err(|e| PlatformError(e.to_string()))?;
        Ok(Self { event_loop, app })
    }
}

struct AndroidRunner<H: EventHandler<AndroidWindow>> {
    handler: H,
    window: Option<AndroidWindow>,
    config: WindowConfig,
    scale_factor: f64,
    modifiers: platform_core::ModifiersState,
    cursor_position: (f64, f64),
    // Last position of an active touch finger, used to emit Scrolled deltas from drag gestures.
    touch: TouchDrag,
    app: AndroidApp,
    // The preference last reported to the app, so a poll only produces an event when it actually changed.
    last_dark: Option<bool>,
    last_theme_poll: Option<std::time::Instant>,
    #[cfg(target_os = "android")]
    choreographer: Option<choreographer::Choreographer>,
    #[cfg(target_os = "android")]
    is_animation_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<H: EventHandler<AndroidWindow>> ApplicationHandler<()> for AndroidRunner<H> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        self.handler.new_events();
        // Polled rather than pushed: the notification this would hang off is the very thing that does not arrive. Done here so the event lands inside the batch `new_events` just opened, like a real input event.
        if self
            .last_theme_poll
            .is_none_or(|at| at.elapsed() >= THEME_POLL_INTERVAL)
        {
            self.last_theme_poll = Some(std::time::Instant::now());
            let dark = prefers_dark(&self.app);
            if dark != self.last_dark {
                self.last_dark = dark;
                // No window yet (or suspended) means nothing to tell: `resumed` re-reads and reports.
                if let (Some(dark), Some(window)) = (dark, self.window.clone()) {
                    self.handler
                        .on_event(Event::ColorSchemeChanged { dark }, &window);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(_d) = self.handler.about_to_wait() {
            #[cfg(target_os = "android")]
            {
                // On Android, use Choreographer vsync callbacks instead of WaitUntil wall-clock timers. This aligns frame wakeups to vsync edges, eliminating jank at any refresh rate (60/90/120 Hz).
                let already_pending = self
                    .is_animation_pending
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
                // ScaleFactorChanged may not fire on first resume, so seed scale_factor here.
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
                // Before the tree mounts, so its first layout is already in the right theme rather than building light and flipping on the next turn.
                self.last_dark = prefers_dark(&self.app);
                self.last_theme_poll = Some(std::time::Instant::now());
                if let Some(dark) = self.last_dark {
                    self.handler
                        .on_event(Event::ColorSchemeChanged { dark }, &window);
                }
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

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.handler.on_suspend();
        // On Android the native window is destroyed on suspend; drop our reference so it can be recreated on resume.
        self.window = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match map_window_event(
            event,
            &mut self.cursor_position,
            &mut self.scale_factor,
            &mut self.modifiers,
            &mut self.touch,
        ) {
            SurfaceIntent::Event(e) => self.handler.on_event(e, &window),
            SurfaceIntent::Dragged(scrolled, moved) => {
                self.handler.on_event(scrolled, &window);
                self.handler.on_event(moved, &window);
            }
            SurfaceIntent::Resized(e) => {
                self.handler.on_event(e, &window);
                window.request_redraw();
            }
            SurfaceIntent::Redraw => self.handler.on_redraw(&window),
            SurfaceIntent::Close(e) => {
                self.handler.on_event(e, &window);
                event_loop.exit();
            }
            SurfaceIntent::Ignore => {}
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
            cursor_position: (0.0, 0.0),
            touch: TouchDrag::default(),
            app: self.app,
            last_dark: None,
            last_theme_poll: None,
            #[cfg(target_os = "android")]
            choreographer,
            #[cfg(target_os = "android")]
            is_animation_pending: animation_pending,
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| PlatformError(e.to_string()))
    }
}
