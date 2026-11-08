//! [`WinitWindow`]: a winit window behind the [`Window`](platform_core::Window) trait.

use std::sync::Arc;

use platform_core::Window as PlatformWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, WindowHandle,
};
use winit::window::Window as WinitInnerWindow;

use crate::DragSeat;

#[derive(Clone)]
/// A winit window behind the [`Window`](platform_core::Window) trait, cloneable as a cheap `Arc` bump.
pub struct WinitWindow {
    window: Arc<WinitInnerWindow>,
    seat: DragSeat,
}

impl WinitWindow {
    /// `window`, starting and taking drags through `seat`.
    pub fn new(window: Arc<WinitInnerWindow>, seat: DragSeat) -> Self {
        seat.attach(&window);
        Self { window, seat }
    }

    /// The winit window itself, for what the trait does not cover.
    pub fn winit(&self) -> &Arc<WinitInnerWindow> {
        &self.window
    }
}

impl HasWindowHandle for WinitWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.window.window_handle()
    }
}

impl HasDisplayHandle for WinitWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.window.display_handle()
    }
}

impl PlatformWindow for WinitWindow {
    fn redraw_waker(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        Some(platform_core::window_waker(self))
    }

    fn width(&self) -> u32 {
        self.window.inner_size().width
    }

    fn height(&self) -> u32 {
        self.window.inner_size().height
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    // A `wl_surface` shows its committed buffer until another is committed. winit's other window systems promise no such thing.
    fn retains_presented_contents(&self) -> bool {
        matches!(
            self.display_handle().map(|handle| handle.as_raw()),
            Ok(RawDisplayHandle::Wayland(_))
        )
    }

    fn drag_window(&self) {
        let _ = self.window.drag_window();
    }

    fn drag_resize_window(&self, edge: platform_core::ResizeEdge) {
        use platform_core::ResizeEdge;
        use winit::window::ResizeDirection;
        let _ = self.window.drag_resize_window(match edge {
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
        self.window.set_decorations(decorated);
    }

    fn set_minimized(&self, minimized: bool) {
        self.window.set_minimized(minimized);
    }

    fn set_maximized(&self, maximized: bool) {
        self.window.set_maximized(maximized);
    }

    fn is_maximized(&self) -> bool {
        self.window.is_maximized()
    }

    fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    fn focus_window(&self) {
        self.window.focus_window();
    }

    /// The purpose goes first because Wayland reads it when the input method is enabled, and the area last because it is dropped while none is.
    fn set_text_input(&self, input: Option<platform_core::TextInput>) {
        use winit::dpi::{LogicalPosition, LogicalSize};
        use winit::window::ImePurpose;
        let Some(input) = input else {
            self.window.set_ime_allowed(false);
            return;
        };
        self.window.set_ime_purpose(match input.secret {
            true => ImePurpose::Password,
            false => ImePurpose::Normal,
        });
        self.window.set_ime_allowed(true);
        let caret = input.caret;
        self.window.set_ime_cursor_area(
            LogicalPosition::new(caret.x, caret.y),
            LogicalSize::new(caret.width.max(1.0), caret.height.max(1.0)),
        );
    }

    fn start_drag(&self, drag: platform_core::OutgoingDrag) -> bool {
        self.seat.start_drag(self.window.id(), drag)
    }

    fn answer_drag(&self, answer: Option<platform_core::DropAnswer>) {
        self.seat.answer_drag(self.window.id(), answer);
    }

    fn show_drag_icon(&self, shown: bool) {
        self.seat.show_drag_icon(shown);
    }

    fn set_cursor(&self, cursor: platform_core::Cursor) {
        use platform_core::Cursor;
        use winit::window::CursorIcon;
        self.window.set_cursor(match cursor {
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
