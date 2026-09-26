//! The surface's primary scroll: the one scroll that stands for the whole page, which a platform can read and move without knowing which widget owns it.
//!
//! A root `ScrollPage` claims it. Every target keeps that scroll where it always has — a canvas, a window or a terminal draws the content at the offset — except a document, which maps it onto its own scroll (see [`DOCUMENT_SCROLL_ATTRIBUTE`]). What reads and writes it through here is platform code that has to treat "where the page is scrolled to" as one value: a location adapter keeping a scroll position per history entry.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The attribute a document backend puts on its host while the primary scroll is the document's own scroll, and the attribute the platform reads to know that the host has grown with the content, so the surface is measured against the viewport rather than the host.
pub const DOCUMENT_SCROLL_ATTRIBUTE: &str = "data-telar-document-scroll";

/// Whatever owns the primary scroll, as the platform sees it.
pub trait PrimaryScroll {
    /// Where the page is scrolled to, in logical pixels.
    fn offset(&self) -> (f32, f32);
    /// Asks for the page to be scrolled to `(x, y)`. Clamped to the content like any other scroll, and applied by the next frame.
    fn scroll_to(&self, x: f32, y: f32);
}

thread_local! {
    static PRIMARY: RefCell<Option<(u64, Rc<dyn PrimaryScroll>)>> = const { RefCell::new(None) };
    static NEXT_CLAIM: Cell<u64> = const { Cell::new(0) };
}

/// Makes `scroll` the primary scroll until the returned claim drops.
///
/// Per thread, and the newest claim wins: a tree rebuilt on the same surface builds its new page before the old one is dropped, and the old one letting go must not take the new one's claim with it.
#[must_use = "the scroll is primary only while the claim is alive"]
pub fn claim_primary_scroll(scroll: Rc<dyn PrimaryScroll>) -> PrimaryScrollClaim {
    let id = NEXT_CLAIM.with(|next| {
        next.set(next.get() + 1);
        next.get()
    });
    PRIMARY.with(|primary| *primary.borrow_mut() = Some((id, scroll)));
    PrimaryScrollClaim { id }
}

/// The primary scroll's offset, or `None` when nothing claimed it.
pub fn primary_scroll_offset() -> Option<(f32, f32)> {
    current().map(|scroll| scroll.offset())
}

/// Scrolls the primary scroll to `(x, y)`, returning `false` when nothing claimed it.
pub fn scroll_primary_to(x: f32, y: f32) -> bool {
    match current() {
        Some(scroll) => {
            scroll.scroll_to(x, y);
            true
        }
        None => false,
    }
}

// Cloned out before calling, so a scroll that reads or claims from inside `offset`/`scroll_to` does not re-enter the borrow.
fn current() -> Option<Rc<dyn PrimaryScroll>> {
    PRIMARY.with(|primary| primary.borrow().as_ref().map(|(_, scroll)| scroll.clone()))
}

/// Holds the primary scroll; dropping it lets go, unless a newer claim has already taken over.
pub struct PrimaryScrollClaim {
    id: u64,
}

impl Drop for PrimaryScrollClaim {
    fn drop(&mut self) {
        PRIMARY.with(|primary| {
            let mut primary = primary.borrow_mut();
            if primary.as_ref().is_some_and(|(id, _)| *id == self.id) {
                *primary = None;
            }
        });
    }
}

#[cfg(test)]
#[path = "primary_scroll_test.rs"]
mod tests;
