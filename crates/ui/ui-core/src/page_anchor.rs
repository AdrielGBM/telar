//! `anchor:` — a box a link can take the reader to: a named place on the page.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use layout_core::NodeId;

use crate::context::track_layout;
use crate::layout_item::LayoutItem;
use crate::link::{AnchorRegistration, anchor_moved, register_anchor};
use crate::scroll_viewports::scroll_viewports_of;

/// Names a box as a place on the page, for [`anchor`](platform_core::anchor) links to reach. Every widget with a layout node takes it.
pub trait PageAnchor: LayoutItem + Sized {
    /// Makes this box the anchor `name`: a link to it reveals it at the top of every scroll it sits in, and adds an entry to the history the way a same-page link on the web does. Re-read when what `name` reads changes, so a name built from the locale follows it.
    ///
    /// A name belongs to one box. A second box registered under it takes it over, with a warning.
    ///
    /// Each target reaches it its own way on top of that: a document gives the element the name as its `id`, so `#name` in the address is a native fragment, and an address opened with a fragment reveals it once the page is there.
    fn page_anchor<S: Into<Arc<str>>>(self, name: impl Fn() -> S + 'static) -> Self {
        let node = self.layout_node();
        let current: Rc<RefCell<Option<Arc<str>>>> = Rc::default();
        let registration: Rc<RefCell<Option<AnchorRegistration>>> = Rc::default();
        let annotation = crate::annotation::slot(node);
        {
            let current = current.clone();
            let registration = registration.clone();
            reactive_core::effect(move || {
                let name: Arc<str> = name().into();
                if current.borrow().as_ref() == Some(&name) {
                    return;
                }
                registration.borrow_mut().take();
                let mut next = annotation.peek();
                next.anchor = Some(name.clone());
                annotation.set(next);
                *registration.borrow_mut() = Some(register_anchor(&name, move || reveal(node)));
                *current.borrow_mut() = Some(name);
            });
        }
        reactive_core::effect(move || {
            if let Some(rect) = track_layout(node) {
                rect.get();
            }
            let name = current.borrow().clone();
            if let Some(name) = name {
                anchor_moved(&name);
            }
        });
        reactive_core::on_cleanup(move || drop(registration.borrow_mut().take()));
        self
    }
}

impl<T: LayoutItem> PageAnchor for T {}

/// Brings `node` to the top of the innermost scroll it sits in, then that scroll to the top of the next one out. `false` while any of them has not been laid out.
fn reveal(node: NodeId) -> bool {
    let mut item = node;
    for viewport in scroll_viewports_of(node) {
        if viewport.reveal_at_start(item).is_none() {
            return false;
        }
        item = viewport.area();
    }
    true
}

#[cfg(test)]
#[path = "page_anchor_test.rs"]
mod tests;
