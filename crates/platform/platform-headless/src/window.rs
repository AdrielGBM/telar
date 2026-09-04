//! A window with no window behind it: a size, a scale factor and nothing to present to.

use std::sync::Arc;

use platform_core::Window;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

/// The one canonical offscreen window marker. It implements [`platform_core::Window`], so a single type satisfies both the renderer bound (which needs only the raw-window-handle traits) and the platform bound (`Window`). Its handles are always [`HandleError::Unavailable`] — there is no surface — so a renderer built against it must use its `new_headless` constructor, and `AppHandler` detects the unavailable handle to build an offscreen renderer. `request_redraw` is a no-op: [`crate::HeadlessPlatform`] drives frames explicitly rather than through a windowing system's redraw queue.
///
/// This replaces the ad-hoc `HeadlessWindow` that lived in `renderer-hardware` and the per-test `struct Fake;` markers that renderer tests each defined for themselves.
#[derive(Clone)]
pub struct HeadlessWindow {
    inner: Arc<Inner>,
}

struct Inner {
    width: u32,
    height: u32,
    scale_factor: f64,
    prefers_dark: Option<bool>,
}

impl HeadlessWindow {
    /// A logical `width`×`height` offscreen surface at scale 1.0 reporting no OS light/dark preference.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_options(width, height, 1.0, None)
    }

    /// Full control over the reported [`Window::scale_factor`] and [`Window::prefers_dark`].
    pub fn with_options(
        width: u32,
        height: u32,
        scale_factor: f64,
        prefers_dark: Option<bool>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                width,
                height,
                scale_factor,
                prefers_dark,
            }),
        }
    }
}

impl HasWindowHandle for HeadlessWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl HasDisplayHandle for HeadlessWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl Window for HeadlessWindow {
    fn redraw_waker(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        Some(platform_core::window_waker(self))
    }

    fn width(&self) -> u32 {
        self.inner.width
    }
    fn height(&self) -> u32 {
        self.inner.height
    }
    fn request_redraw(&self) {}
    fn scale_factor(&self) -> f64 {
        self.inner.scale_factor
    }
    fn prefers_dark(&self) -> Option<bool> {
        self.inner.prefers_dark
    }
    fn is_offscreen(&self) -> bool {
        true
    }
}
