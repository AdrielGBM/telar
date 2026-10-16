//! What every list that builds only the rows on screen and walks them with a cursor shares: the controls of the rows that are built, for the cursor to point a reader at, and where its rows stand, built or not, for scrolling one into view and paging; with `advanced`, what a list or a table shows while it has no rows.

use std::collections::HashMap;
use std::hash::Hash;

use telar::{
    LayoutError, LayoutItem, LayoutStyle, ReactiveList, Rect, RwSignal, ScrollViewport,
    VirtualList, VirtualRows, focus::FocusId, on_cleanup, signal,
};

use crate::shared;

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

/// Each of `rows` with how many there are, which is what a row is keyed by: the place a row says it stands at among them all is its index and their count, so a list that grows or shrinks builds its rows again, and one only reordered keeps what it measured of each.
#[cfg(feature = "advanced")]
pub(crate) fn counted<T: Clone>(rows: &[T]) -> Vec<(T, usize)> {
    rows.iter().map(|row| (row.clone(), rows.len())).collect()
}

/// Where the rows of a list that builds only those on screen stand in its scroll, built or not: each at the list's row height, placed by multiplying it, or, for rows that may differ, each at the height it was measured at once built and at the row height until then.
///
/// Fixed rows are exact: every row stands where multiplying puts it, built or not, so a page or a reveal lands where it says and the scrollbar never moves under the reader, with nothing to watch or remember. Measured rows know a row's height only once it is built, so a page over rows not yet built is counted at the row height and the scrollbar moves as rows are measured, and each built row is watched; a list measures only when its rows can be taller than one line.
#[derive(Clone)]
pub(crate) struct RowPlaces {
    viewport: ScrollViewport,
    row_height: f32,
    header: f32,
    measured: Option<VirtualRows>,
}

impl RowPlaces {
    /// Rows of `viewport` every one `row_height` tall.
    pub(crate) fn fixed(viewport: ScrollViewport, row_height: f32) -> Self {
        Self {
            viewport,
            row_height,
            header: 0.0,
            measured: None,
        }
    }

    /// Rows of `viewport` at least `row_height` tall, measured when `varying`, else [`fixed`](Self::fixed).
    #[cfg(any(feature = "advanced", feature = "workbench"))]
    pub(crate) fn new(viewport: ScrollViewport, row_height: f32, varying: bool) -> Self {
        let measured = varying.then(|| VirtualRows::new(viewport.clone(), row_height));
        Self {
            measured,
            ..Self::fixed(viewport, row_height)
        }
    }

    /// Rows under a header `height` px tall held at the top of the view: a table's column headings.
    #[cfg(feature = "advanced")]
    pub(crate) fn under_header(self, height: f32) -> Self {
        Self {
            measured: self.measured.map(|rows| rows.under_header(height)),
            header: height,
            ..self
        }
    }

    pub(crate) fn viewport(&self) -> &ScrollViewport {
        &self.viewport
    }

    /// Whether the rows are measured, and so may each be a different height.
    #[cfg(feature = "advanced")]
    pub(crate) fn is_measured(&self) -> bool {
        self.measured.is_some()
    }

    /// A row's box at the list's row height: exactly it for fixed rows, and at least it for measured ones, which grow to hold what runs onto more lines, kept off the row's top and bottom edges by the step between rows.
    pub(crate) fn sized(&self, style: LayoutStyle) -> LayoutStyle {
        match self.measured {
            None => style.height(self.row_height),
            Some(_) => style
                .min_height(self.row_height)
                .padding_vertical(shared::gap_sm()),
        }
    }

    /// The list of `source`'s items, building only those on screen and `overscan` rows' height more beyond each edge.
    pub(crate) fn list<Item, Key, S, K, B>(
        &self,
        overscan: usize,
        source: S,
        key: K,
        build: B,
    ) -> Result<ReactiveList, LayoutError>
    where
        Key: Hash + 'static,
        Item: Clone + 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(usize, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        match &self.measured {
            None => VirtualList::new(
                LayoutStyle::column(),
                self.viewport.clone(),
                self.row_height,
                overscan,
                source,
                key,
                build,
            ),
            Some(rows) => VirtualList::measured(
                LayoutStyle::column(),
                rows.clone(),
                overscan as f32 * self.row_height,
                source,
                key,
                build,
            ),
        }
    }

    /// Scrolls the least distance down or up that brings row `index` fully into view below the header. Worked out from where the row stands rather than from a node, because it may not be built; a measured row not yet built is followed again once it is.
    pub(crate) fn reveal(&self, index: usize) {
        match &self.measured {
            Some(rows) => rows.reveal(index, 0.0),
            None => {
                if self.viewport.rect().peek().height <= 0.0 {
                    return;
                }
                let (x, _) = self.viewport.peek_offset();
                let top = index as f32 * self.row_height;
                let band = Rect::new(x, top, 0.0, self.row_height + self.header);
                self.viewport.reveal_rect(band, 0.0);
            }
        }
    }

    /// Where Page Down (`forward`) or Page Up takes the cursor from the row at `from`, or from the header for `None`, among `count` rows: on by as many rows as the view shows whole under the header, counting a row not yet built at the row height, and at least one. `None` is the header, reached going up from a row a page or less below it; a list without one stops at its first row.
    #[cfg(any(feature = "advanced", feature = "workbench"))]
    pub(crate) fn page(&self, from: Option<usize>, count: usize, forward: bool) -> Option<usize> {
        let last = count.checked_sub(1)?;
        let view = self.viewport.rect().peek().height - self.header;
        if forward {
            let reach = from.map_or(self.header, |at| self.top(at) + self.height_of(at)) + view;
            let at = self.row_at(reach, count);
            let at = match self.top(at) + self.height_of(at) > reach {
                true => at.checked_sub(1),
                false => Some(at),
            };
            let next = from.map_or(0, |from| from + 1);
            return Some(at.map_or(next, |at| at.max(next)).min(last));
        }
        let from = from?.min(last);
        let reach = self.top(from) - view;
        if from == 0 || reach <= 0.0 {
            return (self.header <= 0.0).then_some(0);
        }
        let at = self.row_at(reach, count);
        let at = match self.top(at) < reach {
            true => at + 1,
            false => at,
        };
        Some(at.min(from - 1))
    }

    /// How tall a one-line row's content stands: the whole row for fixed rows, and for measured ones what [`sized`](Self::sized) leaves inside its padding.
    #[cfg(feature = "workbench")]
    pub(crate) fn line_height(&self) -> f32 {
        match self.measured {
            None => self.row_height,
            Some(_) => (self.row_height - shared::gap_sm() * 2.0).max(0.0),
        }
    }

    /// How a row's text breaks: onto more lines for measured rows, which grow to hold them, and on one line cut off where the row ends for fixed ones.
    #[cfg(any(feature = "advanced", feature = "workbench"))]
    pub(crate) fn text_wrap(&self) -> telar::TextWrap {
        match self.measured {
            Some(_) => telar::TextWrap::Wrap,
            None => telar::TextWrap::NoWrap,
        }
    }

    #[cfg(any(feature = "advanced", feature = "workbench"))]
    fn top(&self, index: usize) -> f32 {
        match &self.measured {
            Some(rows) => rows.offset_of(index),
            None => self.header + index as f32 * self.row_height,
        }
    }

    #[cfg(any(feature = "advanced", feature = "workbench"))]
    fn height_of(&self, index: usize) -> f32 {
        match &self.measured {
            Some(rows) => rows.height_of(index),
            None => self.row_height,
        }
    }

    #[cfg(any(feature = "advanced", feature = "workbench"))]
    fn row_at(&self, offset: f32, count: usize) -> usize {
        match &self.measured {
            Some(rows) => rows.row_at(offset),
            None if self.row_height <= 0.0 => 0,
            None => (((offset - self.header) / self.row_height).floor().max(0.0) as usize)
                .min(count.saturating_sub(1)),
        }
    }
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
