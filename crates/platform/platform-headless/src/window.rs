//! A window with no window behind it: a size, a scale factor and nothing to present to.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use platform_core::{Cursor, Window};
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
    width: AtomicU32,
    height: AtomicU32,
    scale_factor: f64,
    cursor: Mutex<Cursor>,
}

impl HeadlessWindow {
    /// A logical `width`×`height` offscreen surface at scale 1.0.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_scale_factor(width, height, 1.0)
    }

    /// Full control over the reported [`Window::scale_factor`].
    pub fn with_scale_factor(width: u32, height: u32, scale_factor: f64) -> Self {
        Self {
            inner: Arc::new(Inner {
                width: AtomicU32::new(width),
                height: AtomicU32::new(height),
                scale_factor,
                cursor: Mutex::new(Cursor::Default),
            }),
        }
    }

    /// Gives the window a new size, as a user dragging its edge would. Every clone sees it: they are one window.
    pub fn resize(&self, width: u32, height: u32) {
        self.inner.width.store(width, Ordering::Relaxed);
        self.inner.height.store(height, Ordering::Relaxed);
    }

    /// The pointer shape last requested through [`Window::set_cursor`], so a test can see what a real window would show.
    pub fn cursor(&self) -> Cursor {
        *self
            .inner
            .cursor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
        self.inner.width.load(Ordering::Relaxed)
    }
    fn height(&self) -> u32 {
        self.inner.height.load(Ordering::Relaxed)
    }
    fn request_redraw(&self) {}
    fn scale_factor(&self) -> f64 {
        self.inner.scale_factor
    }
    fn set_cursor(&self, cursor: Cursor) {
        *self
            .inner
            .cursor
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = cursor;
    }
    fn is_offscreen(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "window_test.rs"]
mod tests;
