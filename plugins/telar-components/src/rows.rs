//! What every list that builds only the rows on screen and walks them with a cursor shares: the controls of the rows that are built, for the cursor to point a reader at, and scrolling a row it may not have built into view.

use std::collections::HashMap;
use std::hash::Hash;

use telar::{Rect, RwSignal, ScrollViewport, focus::FocusId, on_cleanup, signal};

#[cfg(any(feature = "advanced", feature = "workbench"))]
pub(crate) use paint::{RowState, cursor_row_paint};

/// The presented control of each row that is built, by the key the list names its rows by: what a cursor resting on a row names to a reader, and nothing for a row scrolled out of the built window.
pub(crate) struct BuiltRows<K: 'static>(RwSignal<HashMap<K, FocusId>>);

impl<K: 'static> Clone for BuiltRows<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: 'static> Copy for BuiltRows<K> {}

impl<K: Clone + Eq + Hash + 'static> BuiltRows<K> {
    pub(crate) fn new() -> Self {
        Self(signal(HashMap::new()))
    }

    /// The control of the row `key` names, while it is built, subscribing the caller to the rows coming and going.
    pub(crate) fn get(&self, key: &K) -> Option<FocusId> {
        self.0.with(|built| built.get(key).copied())
    }

    /// Records `row` as the control of the row `key` names, until the row is dropped. Call it while the row is built, so the withdrawal belongs to it.
    pub(crate) fn track(&self, key: K, row: FocusId) {
        self.0.update(|built| {
            built.insert(key.clone(), row);
        });
        let built = *self;
        on_cleanup(move || built.dropped(&key, row));
    }

    /// A row rebuilt under the same key registers before the old one drops, so only the control recorded now is withdrawn.
    fn dropped(&self, key: &K, row: FocusId) {
        if self.0.is_alive() && self.0.peek_with(|built| built.get(key) == Some(&row)) {
            self.0.update(|built| {
                built.remove(key);
            });
        }
    }
}

/// Scrolls the least distance that brings the band `top..top + height` of `viewport`'s content fully into view, across as well as down left where it is. Worked out from where the band is rather than from a node, because the row it stands for may not be built.
pub(crate) fn reveal_band(viewport: &ScrollViewport, top: f32, height: f32) {
    if viewport.rect().peek().height <= 0.0 {
        return;
    }
    let (x, _) = viewport.peek_offset();
    viewport.reveal_rect(Rect::new(x, top, 0.0, height), 0.0);
}

#[cfg(any(feature = "advanced", feature = "workbench"))]
mod paint {
    use telar::{Border, BorderRadius, Color, Reactive, RectStyle, ShapeStyle};

    use crate::list::tinted;
    use crate::shared;

    /// What a row of a list walked by a cursor is showing: chosen, under the pointer, and where the keyboard's cursor rests while the list shows focus.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct RowState {
        pub(crate) selected: bool,
        pub(crate) hovered: bool,
        pub(crate) cursor: bool,
    }

    /// A row's paint, in a corner of `radius`: the selection's wash, else the pointer's, at the theme's strengths and in the list's `color` when it was given one; and the focus ring round the cursor's row, so the cursor and the selection read apart when they are on different rows.
    pub(crate) fn cursor_row_paint(
        radius: f32,
        color: &Reactive<Color>,
        state: RowState,
    ) -> RectStyle {
        let style = RectStyle::default().with_radius(BorderRadius::all(radius));
        let style = if state.selected {
            style.with_fill(tinted(color, shared::held_wash()))
        } else if state.hovered {
            style.with_fill(tinted(color, shared::hover_wash()))
        } else {
            style
        };
        if !state.cursor {
            return style;
        }
        let ring = shared::focus_ring();
        let ring = match color.get() {
            tone if tone == Color::TRANSPARENT => ring,
            tone => Border {
                paint: tone.into(),
                ..ring
            },
        };
        style.with_border(ring)
    }
}

#[cfg(test)]
#[path = "rows_test.rs"]
mod tests;
