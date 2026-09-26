//! A document's links followed the way a browser follows them, with the one plain click that stays in the app handed back to it.

use platform_core::consumed_keys::FOCUS_BOX_ATTRIBUTE;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

/// Listens for clicks on the app's `<a>` elements.
///
/// The `<a>` is the link: the browser activates it on a click, on Enter and for a screen reader, and opens it in a new tab or window on a modified or middle click, which a tab the app opened itself could never match. Only a plain click on an address this page can go to is taken back, since following it natively would reload the page: its default is prevented and the box is reported as [`Event::BoxActivated`](platform_core::Event::BoxActivated), for the app to follow the destination itself. An `<a>` that opens beside the page or downloads is left entirely to the browser.
pub(crate) fn follow_links(host: &web_sys::HtmlElement) -> Option<LinkFollower> {
    let watched = host.clone();
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        if let Some(box_id) = stays_in_the_app(&watched, &event) {
            event.prevent_default();
            platform_core::post_event(platform_core::Event::BoxActivated { box_id });
        }
    });
    host.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .ok()?;
    Some(LinkFollower {
        host: host.clone(),
        closure,
    })
}

/// The box a click activates, when it is a plain click on one of the app's links to an address on this page's origin.
fn stays_in_the_app(host: &web_sys::HtmlElement, event: &web_sys::Event) -> Option<u64> {
    let click = event.dyn_ref::<web_sys::MouseEvent>()?;
    let modified = click.ctrl_key() || click.meta_key() || click.shift_key() || click.alt_key();
    if event.default_prevented() || click.button() != 0 || modified {
        return None;
    }
    let anchor = event
        .target()?
        .dyn_into::<web_sys::Element>()
        .ok()?
        .closest("a[href]")
        .ok()??;
    if !host.contains(Some(anchor.as_ref())) {
        return None;
    }
    let box_id = anchor.get_attribute(FOCUS_BOX_ATTRIBUTE)?.parse().ok()?;
    let anchor = anchor.dyn_into::<web_sys::HtmlAnchorElement>().ok()?;
    let origin = web_sys::window()?.location().origin().ok()?;
    let in_place = anchor.target().is_empty() && !anchor.has_attribute("download");
    (in_place && anchor.origin() == origin).then_some(box_id)
}

/// The click listener on the host, taken off when the reconcile goes.
pub(crate) struct LinkFollower {
    host: web_sys::HtmlElement,
    closure: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for LinkFollower {
    fn drop(&mut self) {
        let _ = self
            .host
            .remove_event_listener_with_callback("click", self.closure.as_ref().unchecked_ref());
    }
}
