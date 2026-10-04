//! Which place is under a line across the view of a scroll, the primary one above all: the anchor a bar fixed over the page reads to mark the link to where the reader is, and to dress itself like that place.
//!
//! Worked out from Telar's own layout and the scroll's offset, as [`scroll_progress`](crate::scroll_progress) is, so every target answers the same: the document's scroll on web-dom, a scroll area drawn at its offset everywhere else.

use std::sync::Arc;

use layout_core::NodeId;
use reactive_core::{RwSignal, signal};

use crate::context::track_layout;
use crate::scroll_area::ScrollViewport;
use crate::scroll_viewports::{scroll_viewports_of, use_primary_scroll};

reactive_core::surface_local! {
    /// Per surface: node ids are minted per surface, so one list would have windows answer with each other's places.
    slot PLACES: Places = Places::default();
    access with_places, with_places_ref;
    context PlacesContext, PlacesGuard;
}

struct Places {
    named: RwSignal<Vec<Place>>,
    made: u64,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            named: signal(Vec::new()),
            made: 0,
        }
    }
}

#[derive(Clone)]
struct Place {
    node: NodeId,
    name: Arc<str>,
    made: u64,
}

/// Forgets every place, for a tree being replaced wholesale: the next tree is handed the same node ids, and a place left behind would answer for whatever box the next tree builds on one.
pub(crate) fn reset() {
    let named = with_places_ref(|places| places.named);
    if named.is_alive() {
        named.set(Vec::new());
    }
}

/// Records that the box `node` is the place called `name`, until the returned registration drops.
pub(crate) fn name_place(node: NodeId, name: Arc<str>) -> PlaceRegistration {
    let (named, made) = with_places(|places| {
        places.made += 1;
        (places.named, places.made)
    });
    named.update(|places| places.push(Place { node, name, made }));
    PlaceRegistration { named, made }
}

/// Keeps a box named as a place; dropping it withdraws that naming and no other.
pub(crate) struct PlaceRegistration {
    named: RwSignal<Vec<Place>>,
    made: u64,
}

impl Drop for PlaceRegistration {
    fn drop(&mut self) {
        if self.named.is_alive() {
            self.named
                .update(|places| places.retain(|place| place.made != self.made));
        }
    }
}

/// The anchor whose box spans the line `line` px below the top edge of the primary scroll's view, read reactively: [`ScrollViewport::anchor_at`] on the page that holds the primary scroll, or `None` while none does.
///
/// A bar fixed over the page reads it with the bar's own height, to know which section is under it, from anywhere on the surface: whoever reads it runs again as the page scrolls, as places come and go, as their boxes move and when a page takes the primary scroll or lets it go, so a `memo` over it changes only when the place does.
pub fn use_anchor_at(line: f32) -> Option<Arc<str>> {
    use_primary_scroll()?.anchor_at(line)
}

impl ScrollViewport {
    /// The anchor whose box spans the line `line` px below the top edge of this viewport's view, read reactively: the name `anchor:` gave it, or `None` while no place in this scroll spans the line.
    ///
    /// A box spans the line from its top edge to just above its bottom edge, so of two sections that meet at the line the one below it is the one under it. Where several places span it, the one whose top the reader passed last wins, and of two whose tops are level the shorter: the innermost of nested places. A place inside a scroll area nested in this one counts only where that area shows it, and a place outside this scroll, in a layer fixed over it or an overlay, never does.
    pub fn anchor_at(&self, line: f32) -> Option<Arc<str>> {
        let named = with_places_ref(|places| places.named);
        let at = self.offset().1.get() + line;
        named
            .get()
            .into_iter()
            .filter_map(|place| Some((self.span_of(place.node)?, place.name)))
            .filter(|((top, bottom), _)| *top <= at && at < *bottom)
            .max_by(|((top_a, bottom_a), _), ((top_b, bottom_b), _)| {
                top_a.total_cmp(top_b).then(bottom_b.total_cmp(bottom_a))
            })
            .map(|(_, name)| name)
    }

    /// Where `node` runs from top to bottom in this viewport's content, cut to what every scroll area between them shows of it. `None` while it is not laid out, while those areas show none of it, or when it is not in this scroll at all.
    fn span_of(&self, node: NodeId) -> Option<(f32, f32)> {
        let rect = track_layout(node)?.get();
        let (mut top, mut bottom) = (rect.y, rect.y + rect.height);
        for viewport in scroll_viewports_of(node) {
            if viewport.area() == self.area() {
                return Some((top, bottom));
            }
            let window = viewport.rect().get();
            let scrolled = window.y - viewport.offset().1.get();
            top = (top + scrolled).max(window.y);
            bottom = (bottom + scrolled).min(window.y + window.height);
            if bottom <= top {
                return None;
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "anchor_line_test.rs"]
mod tests;
