//! [`WinitWindow`]: a winit window behind the [`Window`](platform_core::Window) trait.

use std::sync::Arc;

use platform_core::Window as PlatformWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, WindowHandle,
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

    // A `wl_surface` shows its committed buffer until another is committed. winit's other window systems promise no such thing.
    fn retains_presented_contents(&self) -> bool {
        matches!(
            self.display_handle().map(|handle| handle.as_raw()),
            Ok(RawDisplayHandle::Wayland(_))
        )
    }

    fn drag_window(&self) {
        let _ = self.0.drag_window();
    }

    fn drag_resize_window(&self, edge: platform_core::ResizeEdge) {
        use platform_core::ResizeEdge;
        use winit::window::ResizeDirection;
        let _ = self.0.drag_resize_window(match edge {
            ResizeEdge::North => ResizeDirection::North,
            ResizeEdge::South => ResizeDirection::South,
            ResizeEdge::East => ResizeDirection::East,
            ResizeEdge::West => ResizeDirection::West,
            ResizeEdge::NorthEast => ResizeDirection::NorthEast,
            ResizeEdge::NorthWest => ResizeDirection::NorthWest,
            ResizeEdge::SouthEast => ResizeDirection::SouthEast,
            ResizeEdge::SouthWest => ResizeDirection::SouthWest,
        });
    }

    fn set_decorations(&self, decorated: bool) {
        self.0.set_decorations(decorated);
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
            Cursor::EwResize => CursorIcon::EwResize,
            Cursor::NsResize => CursorIcon::NsResize,
            Cursor::NwseResize => CursorIcon::NwseResize,
            Cursor::NeswResize => CursorIcon::NeswResize,
            Cursor::Move => CursorIcon::Move,
            Cursor::Text => CursorIcon::Text,
            Cursor::NotAllowed => CursorIcon::NotAllowed,
            Cursor::Wait => CursorIcon::Wait,
        });
    }
}
