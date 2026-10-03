//! A window with no window behind it: a size, a scale factor and nothing to present to.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use platform_core::{Cursor, Window};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

/// Every title a [`HeadlessWindow`] was given, oldest first. Shared, because the window is made inside a run that yields nothing back: a caller hands one in with [`HeadlessPlatform::record_titles_into`](crate::HeadlessPlatform::record_titles_into) and reads it after the run returns.
pub type TitleSink = Arc<Mutex<Vec<String>>>;

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
    title: Mutex<String>,
    titles: Mutex<Option<TitleSink>>,
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
                title: Mutex::new(String::new()),
                titles: Mutex::new(None),
            }),
        }
    }

    /// Gives the window a new size, as a user dragging its edge would. Every clone sees it: they are one window.
    pub fn resize(&self, width: u32, height: u32) {
        self.inner.width.store(width, Ordering::Relaxed);
        self.inner.height.store(height, Ordering::Relaxed);
    }

    /// The title last given through [`Window::set_title`]: what a real window's title bar, a tab or a task switcher would show. Empty until one is given.
    pub fn title(&self) -> String {
        lock(&self.inner.title).clone()
    }

    /// Writes every title this window is given into `sink`, oldest first, for a caller that asserts on — or a prerender that reads — what the surface was called.
    pub fn record_titles_into(&self, sink: TitleSink) {
        *lock(&self.inner.titles) = Some(sink);
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
    fn set_title(&self, title: &str) {
        *lock(&self.inner.title) = title.to_owned();
        if let Some(sink) = lock(&self.inner.titles).as_ref() {
            lock(sink).push(title.to_owned());
        }
    }
    fn is_offscreen(&self) -> bool {
        true
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "window_test.rs"]
mod tests;
