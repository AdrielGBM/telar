//! The canvas a frame is presented on.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, WebDisplayHandle, WebWindowHandle,
};
use renderer_dom::AccessMirror;
use wasm_bindgen::JsCast;

/// Hands out the `data-raw-handle` values wgpu looks a canvas up by. Starts at 1 because 0 reads as "unset" in the DOM attribute and would collide with a canvas somebody else put on the page.
static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);

/// A `<canvas>` a wgpu surface can be created on.
///
/// Identified to wgpu by a `data-raw-handle` attribute rather than by a pointer to the JS object: that is the mechanism every windowing library on the web uses, it survives the element being moved in the DOM, and it needs no lifetime to be threaded through the renderer.
#[derive(Clone)]
pub struct CanvasSurface {
    canvas: web_sys::HtmlCanvasElement,
    handle: u32,
    /// What the page was served with inside the host, shown until the first frame is drawn over it: a prerendered page, readable before the module has loaded.
    served: Rc<RefCell<Vec<web_sys::Node>>>,
    /// What a reader reads in place of the pixels, from the first snapshot with anything in it.
    mirror: Rc<RefCell<Option<AccessMirror>>>,
}

#[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
compile_error!(
    "telar-renderer-web's CanvasSurface Send/Sync impls are only sound without wasm `atomics`; drop `+atomics` from the wasm target features"
);

// SAFETY: wasm32 without `atomics` is single-threaded, so nothing here is observed from another thread.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for CanvasSurface {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for CanvasSurface {}

impl CanvasSurface {
    pub fn canvas(&self) -> &web_sys::HtmlCanvasElement {
        &self.canvas
    }

    /// Sizes the backing store to `width`×`height` **device** pixels while leaving the element's CSS size to the page. Skips the write when the size already matches: assigning `width` clears the canvas, so doing it every frame would throw the last frame away before the new one is drawn.
    pub fn resize(&self, width: u32, height: u32) {
        if self.canvas.width() != width {
            self.canvas.set_width(width);
        }
        if self.canvas.height() != height {
            self.canvas.set_height(height);
        }
    }

    /// Takes away what the page was served with, now that a frame stands in its place. A canvas does not adopt a prerendered document the way the document renderer does; it replaces it.
    pub(crate) fn release_served(&self) {
        for node in self.served.borrow_mut().drain(..) {
            if let Some(parent) = node.parent_node() {
                let _ = parent.remove_child(&node);
            }
        }
    }

    /// Tells assistive technology what the canvas shows, as `nodes` — the window's accessibility snapshot — say it: in a visually hidden [`AccessMirror`] beside the canvas, whose elements hand what a reader does to them back as the platform's events.
    ///
    /// The page the host was served with stays the one a reader reads until the mirror has something to say. From then on it is withdrawn from readers and from the keyboard, though still shown until the first frame is drawn over it, so a reader never meets the page twice.
    pub fn describe(&self, nodes: &[platform_core::AccessNode]) {
        let mut mirror = self.mirror.borrow_mut();
        if mirror.is_none() {
            if nodes.is_empty() {
                return;
            }
            match AccessMirror::before(self.canvas.as_ref()) {
                Ok(made) => *mirror = Some(made),
                Err(e) => {
                    tracing::warn!("the canvas has no accessibility mirror: {e}");
                    return;
                }
            }
            self.withdraw_served();
        }
        if let Some(mirror) = mirror.as_mut() {
            mirror.show(nodes);
        }
    }

    /// Takes what the page was served with away from readers and from the keyboard, handing the surface the focus a reader left in it.
    fn withdraw_served(&self) {
        let host = self
            .canvas
            .parent_element()
            .and_then(|host| host.dyn_into::<web_sys::HtmlElement>().ok());
        let active = host
            .as_ref()
            .and_then(|host| host.owner_document())
            .and_then(|document| document.active_element());
        for node in self.served.borrow().iter() {
            let Some(element) = node.dyn_ref::<web_sys::Element>() else {
                continue;
            };
            if let (Some(active), Some(host)) = (&active, &host)
                && element.contains(Some(active.as_ref()))
            {
                let _ = host.focus();
            }
            let _ = element.set_attribute("inert", "");
            let _ = element.set_attribute("aria-hidden", "true");
        }
    }
}

impl HasWindowHandle for CanvasSurface {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Web(WebWindowHandle::new(self.handle));
        // SAFETY: the handle names an element this struct keeps alive, and the attribute it is looked up by is written once at construction and never changed.
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
/// The canvas rather than the host is what a pixel renderer draws on, and it is created here rather than asked of the application: a page that wants to place one itself passes it to [`CanvasSurface::wrap`](CanvasSurface::wrap) instead.
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
    // The element fills its host in CSS pixels; the *backing store* is sized separately, in device pixels, by `resize`. Keeping the two apart is what makes the frame sharp on a high-density screen.
    let _ = style.set_property("display", "block");
    let _ = style.set_property("width", "100%");
    let _ = style.set_property("height", "100%");
    // A canvas cannot take the keyboard, and if it could it would take it from the host that manages focus.
    let _ = style.set_property("outline", "none");

    let children = host.child_nodes();
    let served = (0..children.length())
        .filter_map(|index| children.item(index))
        .collect();
    host.append_child(&canvas)
        .map_err(|_| "could not add the canvas to its host".to_string())?;
    let surface = CanvasSurface::wrap(canvas);
    surface.served.replace(served);
    Ok(surface)
}

impl CanvasSurface {
    /// Adopts a canvas the page already placed, tagging it so wgpu can find it.
    pub fn wrap(canvas: web_sys::HtmlCanvasElement) -> Self {
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        let _ = canvas.set_attribute("data-raw-handle", &handle.to_string());
        Self {
            canvas,
            handle,
            served: Rc::default(),
            mirror: Rc::default(),
        }
    }
}

/// The browser window, for the probe.
pub(crate) fn dom_window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}
