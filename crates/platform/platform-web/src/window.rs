//! The browser surface, as a window.

use std::cell::Cell;
use std::rc::Rc;

use platform_core::{Cursor, Window};

use crate::dom;

struct Inner {
    host: web_sys::HtmlElement,
    /// CSS pixels — which are the logical pixels layout works in, so nothing is converted.
    width: Cell<u32>,
    height: Cell<u32>,
    scale: Cell<f64>,
}

/// One element of a page, presented as a window.
///
/// Its size is the element's own, in CSS pixels: a Telar layout and a CSS layout agree on what a pixel is,
/// so an app mounted in a 900-pixel column lays out to 900 whether the page put that column there with a
/// media query or a flex rule.
#[derive(Clone)]
pub struct WebWindow {
    inner: Rc<Inner>,
}

impl WebWindow {
    pub fn new(host: web_sys::HtmlElement) -> Self {
        let window = Self {
            inner: Rc::new(Inner {
                host,
                width: Cell::new(0),
                height: Cell::new(0),
                scale: Cell::new(1.0),
            }),
        };
        window.measure();
        window
    }

    /// The element this app fills, for a renderer that needs to put something inside it.
    pub fn host(&self) -> &web_sys::HtmlElement {
        &self.inner.host
    }

    /// Re-reads the host's size and the device pixel ratio, reporting what changed.
    pub fn measure(&self) -> Measured {
        let rect = self.inner.host.get_bounding_client_rect();
        // A host with no height of its own — a bare `<body>` — would lay out to nothing, so it takes the
        // viewport instead. Width is safe to read as-is: a block element always has one.
        let viewport = dom::window();
        let width = rect.width().max(1.0).round() as u32;
        let height = if rect.height() >= 1.0 {
            rect.height().round() as u32
        } else {
            viewport
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(600.0)
                .max(1.0)
                .round() as u32
        };
        let scale = viewport.device_pixel_ratio().max(0.5);
        let measured = Measured {
            size: self.inner.width.get() != width || self.inner.height.get() != height,
            scale: self.inner.scale.get() != scale,
        };
        self.inner.width.set(width);
        self.inner.height.set(height);
        self.inner.scale.set(scale);
        measured
    }

    /// Where a client-space point sits inside the host, in the logical pixels layout uses.
    pub fn to_local(&self, client_x: f64, client_y: f64) -> (f64, f64) {
        let rect = self.inner.host.get_bounding_client_rect();
        (client_x - rect.left(), client_y - rect.top())
    }
}

impl Window for WebWindow {
    fn width(&self) -> u32 {
        self.inner.width.get()
    }

    fn height(&self) -> u32 {
        self.inner.height.get()
    }

    fn request_redraw(&self) {
        crate::platform::request_frame();
    }

    /// The device pixel ratio. A renderer drawing pixels multiplies by it to fill the backing store; one
    /// drawing DOM ignores it, because the browser has already applied it.
    fn scale_factor(&self) -> f64 {
        self.inner.scale.get()
    }

    fn set_title(&self, title: &str) {
        dom::document().set_title(title);
    }

    fn set_cursor(&self, cursor: Cursor) {
        let name = match cursor {
            Cursor::Default => "default",
            Cursor::Pointer => "pointer",
            Cursor::Crosshair => "crosshair",
            Cursor::Grab => "grab",
            Cursor::Grabbing => "grabbing",
            Cursor::ColResize => "col-resize",
            Cursor::RowResize => "row-resize",
            Cursor::Text => "text",
            Cursor::NotAllowed => "not-allowed",
            Cursor::Wait => "wait",
        };
        let _ = self.inner.host.style().set_property("cursor", name);
    }

    fn prefers_dark(&self) -> Option<bool> {
        dom::prefers_dark()
    }

    /// None: a frame is requested through `requestAnimationFrame`, whose callback belongs to the thread that
    /// registered it. The platform installs a process-global waker instead, which reaches the same loop.
    fn redraw_waker(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        None
    }
}

/// What a [`WebWindow::measure`] found had moved since the last one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Measured {
    pub size: bool,
    pub scale: bool,
}
