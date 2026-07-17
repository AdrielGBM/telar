use std::sync::Arc;

use platform_core::Window as PlatformWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::window::Window as WinitInnerWindow;

#[derive(Clone)]
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
        // winit reports the OS theme natively on Windows/macOS; on Linux it is always `None` (winit has no
        // color-scheme integration there) — the desktop adapter supplies the freedesktop-portal fallback.
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
}
