//! Where a dragged item lands in a strip of items, and how to put it there.
//!
//! The whole of drag-to-reorder that is not the caller's: given the laid-out rects of the items in display order and where the pointer is, which slot is this, and what does the list look like once the item goes there. Everything above it — which strip, what a chip looks like, whether a drop may cross into another window — stays with the widget that owns those questions.
//!
//! The two rules here are the two that get written differently every time. A slot is decided by an item's **centre**, not by its edges, so passing the midpoint of a neighbour is what moves the gap rather than reaching its far edge. And a target slot counts positions in the list *before* the move, so moving an item rightwards has to account for the hole it leaves behind — the off-by-one that makes "drag one to the end" land one short.

use geometry_core::Rect;

/// Which way a strip runs. Not [`layout_core::Direction`], which is text direction (LTR/RTL) and answers a different question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// The coordinate of `point` along this axis.
    pub fn of(self, point: (f32, f32)) -> f32 {
        match self {
            Axis::Horizontal => point.0,
            Axis::Vertical => point.1,
        }
    }

    /// The centre of `rect` along this axis.
    pub fn centre(self, rect: &Rect) -> f32 {
        match self {
            Axis::Horizontal => rect.x + rect.width / 2.0,
            Axis::Vertical => rect.y + rect.height / 2.0,
        }
    }
}

/// The slot a pointer at `point` names, given the items' laid-out `rects` in display order: the number of items whose centre it has passed.
///
/// The result indexes the list *as displayed*, so it ranges over `0..=rects.len()` — `len()` meaning "past the last item". Feed it to [`apply_move`], which is what knows that a slot counted before the move is not the index the item ends up at.
///
/// `rects` must be in display order; a strip that lays out its items in a different order than it stores them has to permute before calling, since nothing here can tell the two apart.
pub fn insertion_index(rects: &[Rect], point: (f32, f32), axis: Axis) -> usize {
    let along = axis.of(point);
    rects
        .iter()
        .filter(|rect| axis.centre(rect) < along)
        .count()
        .min(rects.len())
}

/// Moves the item at `from` into slot `to`, where `to` counts positions in `items` **as it is now** — the frame of reference [`insertion_index`] answers in. Returns whether anything moved.
///
/// Dropping an item onto the slot it already occupies (or the one immediately after it, which is the same place once the item is lifted out) is not a move, and reports as such so a caller can skip writing a signal nothing changed.
pub fn apply_move<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() {
        return false;
    }
    let to = to.min(items.len());
    // Removing `from` first shifts every later position down one, so a slot past it is one too far.
    let target = if to > from { to - 1 } else { to };
    if target == from {
        return false;
    }
    let item = items.remove(from);
    items.insert(target, item);
    true
}

#[cfg(test)]
#[path = "reorder_test.rs"]
mod tests;
