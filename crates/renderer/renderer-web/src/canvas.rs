//! The canvas a frame is presented on.

use std::sync::atomic::{AtomicU32, Ordering};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WebDisplayHandle, WebWindowHandle,
};
use wasm_bindgen::JsCast;

/// Hands out the `data-raw-handle` values wgpu looks a canvas up by. Starts at 1 because 0 reads as "unset"
/// in the DOM attribute and would collide with a canvas somebody else put on the page.
static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);

/// A `<canvas>` a wgpu surface can be created on.
///
/// Identified to wgpu by a `data-raw-handle` attribute rather than by a pointer to the JS object: that is
/// the mechanism every windowing library on the web uses, it survives the element being moved in the DOM,
/// and it needs no lifetime to be threaded through the renderer.
#[derive(Clone)]
pub struct CanvasSurface {
    canvas: web_sys::HtmlCanvasElement,
    handle: u32,
}

// SAFETY: `wasm32-unknown-unknown` without the `atomics` feature has exactly one thread, so nothing here can
// be observed from another one — the same argument wgpu itself makes for its `fragile-send-sync-non-atomic-wasm`
// feature, which this crate's build turns on. The bound exists because the renderer is generic over window
// types that, on every other target, really do cross threads.
unsafe impl Send for CanvasSurface {}
unsafe impl Sync for CanvasSurface {}

impl CanvasSurface {
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    /// Sizes the backing store to `width`×`height` **device** pixels while leaving the element's CSS size to
    /// the page. Skips the write when the size already matches: assigning `width` clears the canvas, so
    /// doing it every frame would throw the last frame away before the new one is drawn.
    pub fn resize(&self, width: u32, height: u32) {
        if self.canvas.width() != width {
            self.canvas.set_width(width);
        }
        if self.canvas.height() != height {
            self.canvas.set_height(height);
        }
    }
}

impl HasWindowHandle for CanvasSurface {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Web(WebWindowHandle::new(self.handle));
        // SAFETY: the handle names an element this struct keeps alive, and the attribute it is looked up by
        // is written once at construction and never changed.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for CanvasSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = RawDisplayHandle::Web(WebDisplayHandle::new());
        // SAFETY: the web display handle carries no pointer and names nothing that can go away.
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

/// Puts a canvas inside `host`, filling it.
///
/// The canvas rather than the host is what a pixel renderer draws on, and it is created here rather than
/// asked of the application: a page that wants to place one itself passes it to
/// [`CanvasSurface::wrap`](CanvasSurface::wrap) instead.
pub fn canvas_in(host: &web_sys::HtmlElement) -> Result<CanvasSurface, String> {
    let document = host
        .owner_document()
        .ok_or_else(|| "the host element is not in a document".to_string())?;
    let canvas = document
        .create_element("canvas")
        .map_err(|_| "could not create a canvas".to_string())?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "created element is not a canvas".to_string())?;

    let style = canvas.style();
    // The element fills its host in CSS pixels; the *backing store* is sized separately, in device pixels,
    // by `resize`. Keeping the two apart is what makes the frame sharp on a high-density screen.
    let _ = style.set_property("display", "block");
    let _ = style.set_property("width", "100%");
    let _ = style.set_property("height", "100%");
    // A canvas cannot take the keyboard, and if it could it would take it from the host that manages focus.
    let _ = style.set_property("outline", "none");

    host.append_child(&canvas)
        .map_err(|_| "could not add the canvas to its host".to_string())?;
    Ok(CanvasSurface::wrap(canvas))
}

impl CanvasSurface {
    /// Adopts a canvas the page already placed, tagging it so wgpu can find it.
    pub fn wrap(canvas: web_sys::HtmlCanvasElement) -> Self {
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        let _ = canvas.set_attribute("data-raw-handle", &handle.to_string());
        Self { canvas, handle }
    }
}

/// The browser window, for the probe.
pub(crate) fn dom_window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}
