use std::sync::Arc;

use platform_core::{Window as PlatformWindow};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::window::Window as WinitInnerWindow;

#[derive(Clone)]
pub struct WinitWindow(pub(crate) Arc<WinitInnerWindow>);

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
}
