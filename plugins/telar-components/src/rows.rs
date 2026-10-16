//! What every list that builds only the rows on screen and walks them with a cursor shares: the controls of the rows that are built, for the cursor to point a reader at, and scrolling a row it may not have built into view; with `advanced`, what a list or a table shows while it has no rows.

use std::collections::HashMap;
use std::hash::Hash;

use telar::{Rect, RwSignal, ScrollViewport, focus::FocusId, on_cleanup, signal};

#[cfg(feature = "advanced")]
pub(crate) use empty::with_empty_state;
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

#[cfg(feature = "advanced")]
mod empty {
    use std::rc::Rc;

    use telar::{
        AlignItems, Children, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
        Reactive, Text, box_item, focus::Role,
    };

    use crate::{shared, strings};

    /// What a list or a table that has no rows is wrapped in to say so: `scroll`, the list itself, and under it a status that holds, while `empty` answers `true`, `children` where there are any, else `label`, else the localized `telar_components.nothing_to_show`.
    ///
    /// While empty, the list keeps only `kept` px, its header row or for a list without one a row's height, so it still has a box to hold focus in, and the message takes the rest, centred. Beside the list rather than over it, since a list box or a grid holds nothing but its rows, and a reader hears the message as its own; and a status that is there all along, so a reader hears it when the last row goes.
    pub(crate) fn with_empty_state(
        scroll: Box<dyn LayoutItem>,
        empty: impl Fn() -> bool + 'static,
        kept: f32,
        label: Option<Reactive<String>>,
        children: Children,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let empty: Rc<dyn Fn() -> bool> = Rc::new(empty);
        let frame = {
            let empty = empty.clone();
            move || match empty() {
                true => LayoutStyle::column().height(kept).flex_shrink(0.0),
                false => LayoutStyle::column().flex_grow(1.0).min_height(0.0),
            }
        };
        let rows = Container::new(frame(), vec![scroll])?.styled_by(frame);
        let shown = empty.clone();
        let message = shared::shown_while(
            move || shown(),
            LayoutStyle::column(),
            move || {
                let mut content = children.build_slot(None)?;
                if content.is_empty() {
                    let label = label.clone();
                    let text = Text::declaring(
                        move || match &label {
                            Some(label) => label.get(),
                            None => strings::text(strings::NOTHING_TO_SHOW),
                        },
                        LayoutStyle::new(),
                        |t| shared::quiet(t, 1.0),
                    )?;
                    content.push(box_item(text));
                }
                let message = Container::new(
                    LayoutStyle::column()
                        .align_items(AlignItems::CENTER)
                        .gap(shared::gap_md()),
                    content,
                )?;
                Ok(box_item(message))
            },
        )?;
        let status_box = move || match empty() {
            true => LayoutStyle::column()
                .flex_grow(1.0)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_all(shared::gap_md()),
            false => LayoutStyle::column(),
        };
        let status = Container::new(status_box(), vec![message])?
            .styled_by(status_box)
            .role(Role::Status);
        let root = Container::new(
            LayoutStyle::column().flex_grow(1.0).min_height(0.0),
            vec![box_item(rows), box_item(status)],
        )?;
        Ok(box_item(root))
    }
}

#[cfg(test)]
#[path = "rows_test.rs"]
mod tests;
