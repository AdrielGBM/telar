//! The surface's primary scroll, mapped onto the document's own scroll.
//!
//! A box inside the page scrolls as an `overflow: auto` element. The primary scroll is the page itself, so it gets the scroll a person expects of a page: the host grows with the content and the document scrolls, which brings the browser's own bar, the mobile address bar collapsing, keyboard and anchor scrolling of the whole page, and a position the browser keeps on its own before the app has loaded.

use std::cell::Cell;
use std::rc::Rc;

use platform_core::primary_scroll::DOCUMENT_SCROLL_ATTRIBUTE;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

/// Host properties the document scroll overrides, and what they must be for it: a host that keeps a fixed height stops the page from growing, and one that declines touch gestures stops a finger from scrolling it, because a touch pans only when every box between it and the scroller allows it. A page scrolls down only, like every other `ScrollPage`, so content wider than it is clipped rather than handed to the document as a sideways scroll; `clip`, not `hidden`, because `hidden` would make the host a scroller and take the scroll away from the document.
const HOST_OVERRIDES: [(&str, &str); 3] = [
    ("height", "auto"),
    ("touch-action", "pan-x pan-y"),
    ("overflow-x", "clip"),
];

/// The document scroll while one box holds it. Dropping it gives the host back as it was found.
pub(crate) struct DocumentScroll {
    host: web_sys::HtmlElement,
    box_id: Rc<Cell<u64>>,
    listener: Closure<dyn FnMut(web_sys::Event)>,
    found: Vec<(&'static str, String)>,
    /// Where the document was when the scroll was taken, until the frame that took it has finished changing the page.
    arrived_at: Cell<Option<(f64, f64)>>,
}

impl DocumentScroll {
    /// Hands the document scroll to box `id`, reporting where the document already is so a page scrolled before the app loaded stays where the reader put it.
    pub(crate) fn hold(host: &web_sys::HtmlElement, id: u64) -> Self {
        let style = host.style();
        let found = HOST_OVERRIDES
            .iter()
            .map(|(name, value)| {
                let previous = style.get_property_value(name).unwrap_or_default();
                let _ = style.set_property(name, value);
                (*name, previous)
            })
            .collect();
        let _ = host.set_attribute(DOCUMENT_SCROLL_ATTRIBUTE, "");
        let box_id = Rc::new(Cell::new(id));
        let reported = box_id.clone();
        let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            report(reported.get());
        });
        let options = web_sys::AddEventListenerOptions::new();
        options.set_passive(true);
        let _ = window().add_event_listener_with_callback_and_add_event_listener_options(
            "scroll",
            listener.as_ref().unchecked_ref(),
            &options,
        );
        let arrived_at = offset();
        report_at(id, arrived_at);
        Self {
            host: host.clone(),
            box_id,
            listener,
            found,
            arrived_at: Cell::new(Some(arrived_at)),
        }
    }

    /// Puts the document back where it was when the scroll was taken, once the frame that took it is done.
    ///
    /// That frame replaces whatever the host held before the app loaded, and the browser's scroll anchoring reads new content arriving above the old as content pushed in over the reader, moving the page down by all of it.
    pub(crate) fn keep_arrival(&self) {
        if let Some((x, y)) = self.arrived_at.take() {
            settle(Some((x as f32, y as f32)));
        }
    }

    /// Scrolls the document where the primary scroll is asked to be, which overrides where it arrived.
    pub(crate) fn scroll_as_asked(&self, scroll_to: Option<(f32, f32)>) {
        if scroll_to.is_some() {
            self.arrived_at.set(None);
            settle(scroll_to);
        }
    }

    /// Moves the document scroll to box `id`, a page rebuilt under the same surface.
    pub(crate) fn follow(&self, id: u64) {
        if self.box_id.replace(id) != id {
            report(id);
        }
    }

    /// Where the surface's origin is in the viewport, for what the app places against the surface rather than inside the page: an overlay stays put while the page scrolls under it.
    pub(crate) fn surface_origin(&self) -> (f32, f32) {
        let rect = self.host.get_bounding_client_rect();
        let (x, y) = offset();
        ((rect.left() + x) as f32, (rect.top() + y) as f32)
    }
}

impl Drop for DocumentScroll {
    fn drop(&mut self) {
        let _ = window()
            .remove_event_listener_with_callback("scroll", self.listener.as_ref().unchecked_ref());
        let style = self.host.style();
        for (name, value) in &self.found {
            if value.is_empty() {
                let _ = style.remove_property(name);
            } else {
                let _ = style.set_property(name, value);
            }
        }
        let _ = self.host.remove_attribute(DOCUMENT_SCROLL_ATTRIBUTE);
    }
}

/// Scrolls the document where the primary scroll is asked to be. Compared first, like a box's own scroll, because the request is a frame late for a scroll the reader is already making.
fn settle(scroll_to: Option<(f32, f32)>) {
    let Some((x, y)) = scroll_to else {
        return;
    };
    let (at_x, at_y) = offset();
    if (at_x as f32 - x).abs() >= 0.5 || (at_y as f32 - y).abs() >= 0.5 {
        window().scroll_to_with_x_and_y(x as f64, y as f64);
    }
}

fn report(box_id: u64) {
    report_at(box_id, offset());
}

fn report_at(box_id: u64, (x, y): (f64, f64)) {
    platform_core::post_event(platform_core::Event::BoxScrolled {
        box_id,
        x: x as f32,
        y: y as f32,
    });
}

fn offset() -> (f64, f64) {
    let window = window();
    (
        window.scroll_x().unwrap_or_default(),
        window.scroll_y().unwrap_or_default(),
    )
}

fn window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}
