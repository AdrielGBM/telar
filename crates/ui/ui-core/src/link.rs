//! Following a [`Destination`]: a route is pushed onto the app's history, an anchor is revealed where it is, and an external URI is handed to the system.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use layout_core::NodeId;
use platform_core::{Destination, ModifiersState};
use rustc_hash::FxHashMap;

type Reveal = Rc<dyn Fn() -> bool>;
type ReadDestination = Rc<dyn Fn() -> Option<Destination>>;

thread_local! {
    static ANCHORS: RefCell<FxHashMap<Arc<str>, Reveal>> = RefCell::default();
    static LINKS: RefCell<FxHashMap<NodeId, ReadDestination>> = RefCell::default();
}

/// Goes where `destination` points, in place: pushes the route, reveals the anchor or opens the URI. `false` when nothing went anywhere — a route with no page, an anchor no box answers to, a URI nothing opened.
pub fn follow(destination: &Destination) -> bool {
    match destination {
        Destination::Route(location) => platform_core::push_location(location.clone()),
        Destination::Anchor(name) => reveal_anchor(name),
        Destination::External(uri) => services_core::open_uri(uri.as_str()),
    }
}

/// Goes where `destination` points in a view beside this one where the target has one — a new browser tab for a route — and in place where it does not.
pub fn follow_beside(destination: &Destination) -> bool {
    if let Destination::Route(location) = destination
        && services_core::open_beside(&platform_core::location_format().format(location))
    {
        return true;
    }
    follow(destination)
}

/// Follows `destination` the way a press with `modifiers` held asks to: Ctrl, Cmd or Shift ask for a view beside this one, as they do on a web page.
pub fn follow_pressed(destination: &Destination, modifiers: ModifiersState) -> bool {
    if modifiers.is_ctrl || modifiers.is_meta || modifiers.is_shift {
        follow_beside(destination)
    } else {
        follow(destination)
    }
}

/// Makes `name` an anchor a link can reveal, until the returned registration drops. A later registration of the same name replaces an earlier one.
///
/// The lookup `anchor:` (T-1.4) fills. Until a box registers under a name, following an anchor to it does nothing and reports `false`.
#[must_use = "the anchor is withdrawn when the registration drops"]
pub fn register_anchor(name: &str, reveal: impl Fn() -> bool + 'static) -> AnchorRegistration {
    let name: Arc<str> = name.into();
    let reveal: Reveal = Rc::new(reveal);
    ANCHORS.with(|anchors| anchors.borrow_mut().insert(name.clone(), reveal.clone()));
    AnchorRegistration { name, reveal }
}

/// Reveals the anchor named `name`, reporting whether one was registered under it.
pub fn reveal_anchor(name: &str) -> bool {
    let reveal = ANCHORS.with(|anchors| anchors.borrow().get(name).cloned());
    reveal.is_some_and(|reveal| reveal())
}

/// Keeps an anchor registered; see [`register_anchor`].
pub struct AnchorRegistration {
    name: Arc<str>,
    reveal: Reveal,
}

impl Drop for AnchorRegistration {
    fn drop(&mut self) {
        ANCHORS.with(|anchors| {
            let mut anchors = anchors.borrow_mut();
            if anchors
                .get(&self.name)
                .is_some_and(|current| Rc::ptr_eq(current, &self.reveal))
            {
                anchors.remove(&self.name);
            }
        });
    }
}

/// Follows the link on the box `box_id` names, for a surface that activated it by itself (see [`Event::BoxActivated`](platform_core::Event::BoxActivated)). `false` when the box is gone, is no link, or has nowhere to go right now.
pub fn activate_box(box_id: u64) -> bool {
    let read = LINKS.with(|links| links.borrow().get(&NodeId::from(box_id)).cloned());
    read.and_then(|read| read())
        .is_some_and(|destination| follow(&destination))
}

pub(crate) fn register_link(node: NodeId, read: ReadDestination) {
    LINKS.with(|links| links.borrow_mut().insert(node, read));
}

pub(crate) fn unregister_link(node: NodeId) {
    LINKS.with(|links| links.borrow_mut().remove(&node));
}

/// Whether the surface follows links by itself. A document does: its `<a>` answers the click, Enter and a reader, opens a new tab on a modified click, and reports a plain one back as [`Event::BoxActivated`](platform_core::Event::BoxActivated). Following it here as well would go twice.
pub(crate) fn surface_follows_links() -> bool {
    ui_tree::element_capture()
}

#[cfg(test)]
#[path = "link_test.rs"]
mod tests;
