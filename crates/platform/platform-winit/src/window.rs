//! [`WinitWindow`]: a winit window behind the [`Window`](platform_core::Window) trait.

use std::sync::Arc;

use platform_core::Window as PlatformWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::window::Window as WinitInnerWindow;

#[derive(Clone)]
/// A winit window behind the [`Window`](platform_core::Window) trait, cloneable as a cheap `Arc` bump.
pub struct WinitWindow(pub Arc<WinitInnerWindow>);

impl HasWindowHandle for WinitWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for WinitWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

impl PlatformWindow for WinitWindow {
    fn redraw_waker(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        Some(platform_core::window_waker(self))
    }

    fn width(&self) -> u32 {
        self.0.inner_size().width
    }

    fn height(&self) -> u32 {
        self.0.inner_size().height
    }

    fn request_redraw(&self) {
        self.0.request_redraw();
    }

    fn scale_factor(&self) -> f64 {
        self.0.scale_factor()
    }

    fn prefers_dark(&self) -> Option<bool> {
        // winit reports the OS theme natively on Windows/macOS; on Linux it is always `None` (winit has no color-scheme integration there) — the desktop adapter supplies the freedesktop-portal fallback.
        self.0.theme().map(|t| t == winit::window::Theme::Dark)
    }

    fn drag_window(&self) {
        let _ = self.0.drag_window();
    }

    fn set_minimized(&self, minimized: bool) {
        self.0.set_minimized(minimized);
    }

    fn set_maximized(&self, maximized: bool) {
        self.0.set_maximized(maximized);
    }

    fn is_maximized(&self) -> bool {
        self.0.is_maximized()
    }

    fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }

    fn focus_window(&self) {
        self.0.focus_window();
    }

    fn set_cursor(&self, cursor: platform_core::Cursor) {
        use platform_core::Cursor;
        use winit::window::CursorIcon;
        self.0.set_cursor(match cursor {
            Cursor::Default => CursorIcon::Default,
            Cursor::Pointer => CursorIcon::Pointer,
            Cursor::Crosshair => CursorIcon::Crosshair,
            Cursor::Grab => CursorIcon::Grab,
            Cursor::Grabbing => CursorIcon::Grabbing,
            Cursor::ColResize => CursorIcon::ColResize,
            Cursor::RowResize => CursorIcon::RowResize,
            Cursor::Text => CursorIcon::Text,
            Cursor::NotAllowed => CursorIcon::NotAllowed,
            Cursor::Wait => CursorIcon::Wait,
        });
    }
}
