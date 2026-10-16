//! [`VirtualList`]: a reactive list that builds only the rows inside its viewport, plus an overscan band.

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::{RwSignal, after_settle, batch, effect, signal};
use renderer_core::RectStyle;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use crate::context::{surface_size, track_layout};
use crate::height_index::HeightIndex;
use crate::layout_item::LayoutItem;
use crate::reactive_list::ReactiveList;
use crate::scroll_area::ScrollViewport;
use crate::styled_container::StyledContainer;

/// Which slice of a long list is worth building, given where the viewport currently is.
///
/// Returned as a half-open `[first, last)` over item indices. `overscan` rows are added on each side so a scroll reveals a row that already exists rather than one built during the frame it appears — the difference between a list that scrolls and one that hitches at every boundary.
///
/// A row height of zero (or a viewport not yet laid out) yields the whole range: a virtual list that guessed "nothing is visible" from a missing measurement would render an empty list on its first frame, which reads as a bug rather than as a not-yet-measured layout.
///
/// [`HeightIndex::window`] is the same question for rows of differing heights.
pub fn visible_window(
    offset: f32,
    viewport_height: f32,
    row_height: f32,
    count: usize,
    overscan: usize,
) -> (usize, usize) {
    if row_height <= 0.0 || viewport_height <= 0.0 || count == 0 {
        return (0, count);
    }
    let first_visible = (offset / row_height).floor().max(0.0) as usize;
    let rows_on_screen = (viewport_height / row_height).ceil() as usize + 1;
    let first = first_visible.saturating_sub(overscan);
    let last = first_visible
        .saturating_add(rows_on_screen)
        .saturating_add(overscan)
        .min(count);
    (first.min(count), last)
}

/// One row of a virtualised list, or the space standing in for the rows that were not built.
enum Slot<Item> {
    /// The run of skipped rows above or below the window, as a single box of their combined height. Keeping the scrollable content the full height is what makes the scrollbar and the wheel behave as if every row were there — which, as far as the user is concerned, they are.
    Gap {
        before: bool,
        height: f32,
    },
    Row(usize, Item),
}

/// A keyed list that builds only the rows currently on screen.
///
/// [`ReactiveList`] builds every item it is given, which is right until the list is long: a wallpaper grid or a full application list pays for thousands of widgets to show a dozen. This builds the visible window plus a little overscan and represents the rest as two spacer boxes, so the content keeps its true height and the scrollbar keeps telling the truth.
///
/// Two constructors, for two kinds of row:
///
/// - [`new`](Self::new) — **fixed row height**, the fast path. Every row must be `row_height` tall, and the window is worked out by division, with nothing to measure or remember. It is what a `.rsx` `virtual row_height:` loop builds, and the right choice whenever the rows are uniform.
/// - [`measured`](Self::measured) — **variable row height**. Rows that have never been built stand at an estimate, each built row is measured once layout has placed it, and the measurement replaces the estimate in a [`HeightIndex`], so finding the rows on screen stays `O(log n)` however long the list. While the estimates are corrected the first row on screen holds still: the scroll offset moves by however much the rows above it turned out taller or shorter than guessed. Bringing a row into view goes through the list's [`VirtualRows`] handle, which knows where unbuilt rows stand.
///
/// `source` still returns every item. That is deliberate: producing a `Vec` of plain data is cheap, and the expensive part — constructing widgets, decoding images, laying out text — is what this defers. A source that is itself expensive should be memoised by the caller, as it would be for any list.
pub struct VirtualList;

impl VirtualList {
    /// A list whose every row is `row_height` tall, building the rows on screen and `overscan` more on each side.
    ///
    /// `viewport` is the enclosing scroll area's live window (see [`crate::LayoutScrollArea::new_with`]). `build` constructs one row and receives its index alongside the item, since a virtualised row often wants to know where it sits.
    // `VirtualList` is only a name for this constructor, which builds a `ReactiveList`; `VirtualList::new` is public API and what the transpiler emits for a `virtual` loop.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<Item, Key, S, K, B>(
        container_style: LayoutStyle,
        viewport: ScrollViewport,
        row_height: f32,
        overscan: usize,
        source: S,
        key: K,
        build: B,
    ) -> Result<ReactiveList, LayoutError>
    where
        Key: Hash + 'static,
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(usize, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        let (_, offset_y) = viewport.offset();
        let rect = viewport.rect();
        let windowed = move || {
            let items = source();
            let count = items.len();
            let (first, last) = visible_window(
                offset_y.get(),
                rect.get().height,
                row_height,
                count,
                overscan,
            );
            let mut slots: Vec<Slot<Item>> = Vec::with_capacity(last - first + 2);
            if first > 0 {
                slots.push(Slot::Gap {
                    before: true,
                    height: first as f32 * row_height,
                });
            }
            slots.extend(
                items
                    .into_iter()
                    .enumerate()
                    .skip(first)
                    .take(last - first)
                    .map(|(at, item)| Slot::Row(at, item)),
            );
            if last < count {
                slots.push(Slot::Gap {
                    before: false,
                    height: (count - last) as f32 * row_height,
                });
            }
            slots
        };

        let keyer = move |slot: &Slot<Item>| match slot {
            Slot::Gap { before, height } => gap_key(*before, *height),
            Slot::Row(at, item) => row_key(*at, hash_key(&key(item))),
        };

        ReactiveList::with_style(container_style, windowed, keyer, move |slot| match slot {
            Slot::Gap { height, .. } => spacer(height),
            Slot::Row(at, item) => build(at, item),
        })
    }

    /// A list whose rows may each be a different height, building the rows on screen and `overscan` px more beyond each edge.
    ///
    /// `rows` is the list's handle onto its heights and its viewport: make it with [`VirtualRows::new`] before building the list, keep a clone wherever a row has to be brought into view, and give each handle to one list only. Rows are stacked as they come, so the column `container_style` lays out should have no `gap` — spacing belongs inside the row, where measuring it counts it.
    ///
    /// A row's height is its laid-out box, margins excluded, and it is remembered by the row's key, so a row that leaves the window and comes back, or moves when the list is filtered, keeps the height it was measured at. A row whose content changes height after it was built is measured again.
    ///
    /// Items are cloned out of the list the source last returned as their rows are built, which is why `Item` must be [`MeasuredItem`] (any `Clone` type) here and not in [`new`](Self::new): the source runs only when what it reads changes, not on every scroll.
    ///
    /// Until the viewport is laid out it builds a surface's height of rows rather than every row: nothing taller than the surface can be on screen at once, and building and measuring a hundred thousand rows to learn that would cost what virtualising saves. Only where the surface's size is not known either does it build them all, as [`new`](Self::new) does.
    pub fn measured<Item, Key, S, K, B>(
        container_style: LayoutStyle,
        rows: VirtualRows,
        overscan: f32,
        source: S,
        key: K,
        build: B,
    ) -> Result<ReactiveList, LayoutError>
    where
        Key: Hash + 'static,
        Item: MeasuredItem,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(usize, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        let rows = rows.0;
        let items: Rc<RefCell<Vec<Item>>> = Rc::default();
        {
            let (items, rows) = (Rc::clone(&items), Rc::clone(&rows));
            effect(move || {
                let fresh = source();
                let keys = fresh.iter().map(|item| hash_key(&key(item))).collect();
                *items.borrow_mut() = fresh;
                rows.adopt(keys);
            });
        }

        let (_, offset_y) = rows.viewport.offset();
        let rect = rows.viewport.rect();
        let windowed = {
            let rows = Rc::clone(&rows);
            move || {
                rows.revision.get();
                let offset = offset_y.get();
                let viewport_height = match rect.get().height {
                    laid_out if laid_out > 0.0 => laid_out,
                    _ => surface_size().height,
                };
                let heights = rows.heights.borrow();
                let keys = rows.keys.borrow();
                let items = items.borrow();
                let count = items.len().min(heights.len());
                let (first, last) = heights.window(offset, viewport_height, overscan);
                let last = last.min(count);
                let first = first.min(last);
                let mut slots = Vec::with_capacity(last - first + 2);
                if first > 0 {
                    slots.push(Slot::Gap {
                        before: true,
                        height: heights.span(0, first),
                    });
                }
                slots.extend((first..last).map(|at| Slot::Row(at, (keys[at], items[at].clone()))));
                if last < heights.len() {
                    slots.push(Slot::Gap {
                        before: false,
                        height: heights.span(last, heights.len()),
                    });
                }
                slots
            }
        };

        let keyer = |slot: &Slot<(u64, Item)>| match slot {
            Slot::Gap { before, height } => gap_key(*before, *height),
            Slot::Row(at, (key, _)) => row_key(*at, *key),
        };

        ReactiveList::with_style(container_style, windowed, keyer, move |slot| match slot {
            Slot::Gap { height, .. } => spacer(height),
            Slot::Row(at, (key, item)) => {
                let row = build(at, item)?;
                rows.watch(at, key, row.layout_node());
                Ok(row)
            }
        })
    }
}

/// What [`VirtualList::measured`] asks of its items: they are cloned out of the list the source last returned as their rows are built. A trait of its own so that a type without `Clone` is told what the list needed it for.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be the item of a measured `virtual` list: its rows are built from clones of the items, so it must implement `Clone`",
    label = "add `#[derive(Clone)]` to this type, or iterate a `Clone` projection of it"
)]
pub trait MeasuredItem: Clone + 'static {}

impl<Item: Clone + 'static> MeasuredItem for Item {}

/// A gap keys on its height so a scroll that changes it rebuilds the spacer.
fn gap_key(before: bool, height: f32) -> String {
    format!("gap:{before}:{height}")
}

/// A row keys on the caller's key and its index, because the same item at a new index sits at a new height.
fn row_key(at: usize, key: u64) -> String {
    format!("row:{at}:{key}")
}

fn hash_key<Key: Hash>(key: &Key) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn spacer(height: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new().height(height).flex_shrink(0.0),
        |_: Rect| RectStyle::default(),
        Vec::new(),
    )?))
}

/// The rows of a [`VirtualList::measured`] list as far as their heights go: where each one stands, built or not, and how to bring one into view.
///
/// A clone is the same handle. Reads answer what is known now — measured heights for rows that have been built, the estimate for the rest — without subscribing, so they are safe to call from the effect that follows a selection; the positions they give move as rows are measured.
#[derive(Clone)]
pub struct VirtualRows(Rc<Rows>);

struct Rows {
    viewport: ScrollViewport,
    estimate: f32,
    heights: RefCell<HeightIndex>,
    keys: RefCell<Vec<u64>>,
    measured: RefCell<HashMap<u64, f32>>,
    /// Bumped whenever a height changes or the rows are replaced, so the window is worked out again.
    revision: RwSignal<u64>,
    /// A row asked to be brought into view before it had been measured, and the margin it asked for.
    pending: Cell<Option<(usize, f32)>>,
    settling: Cell<bool>,
}

impl VirtualRows {
    /// Rows scrolled by `viewport`, the enclosing scroll area's live window (see [`crate::LayoutScrollArea::new_with`]), each standing `estimate` px tall until it is built and measured.
    ///
    /// The closer the estimate is to a typical row, the less the scrollbar moves as rows are measured; the first row on screen holds still either way.
    pub fn new(viewport: ScrollViewport, estimate: f32) -> Self {
        Self(Rc::new(Rows {
            viewport,
            estimate: estimate.max(0.0),
            heights: RefCell::default(),
            keys: RefCell::default(),
            measured: RefCell::default(),
            revision: signal(0),
            pending: Cell::new(None),
            settling: Cell::new(false),
        }))
    }

    /// How many rows the list has.
    pub fn len(&self) -> usize {
        self.0.heights.borrow().len()
    }

    /// Whether the list has no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Where row `index` starts in the scroll's content.
    pub fn offset_of(&self, index: usize) -> f32 {
        self.0.heights.borrow().offset_of(index)
    }

    /// How tall row `index` is: measured once it has been built, the estimate before.
    pub fn height_of(&self, index: usize) -> f32 {
        self.0.heights.borrow().height(index)
    }

    /// The row standing at `offset` in the scroll's content.
    pub fn row_at(&self, offset: f32) -> usize {
        self.0.heights.borrow().row_at(offset)
    }

    /// The height of every row together: the content the scroll scrolls.
    pub fn total_height(&self) -> f32 {
        self.0.heights.borrow().total()
    }

    /// Whether row `index` has been measured rather than estimated.
    pub fn is_measured(&self, index: usize) -> bool {
        self.0.is_measured(index)
    }

    /// Scrolls the least distance that brings row `index` fully into view, leaving `margin` px at the edge it arrived from — [`ScrollViewport::reveal`] for a row that may not be built.
    ///
    /// A row not yet measured is brought to where its estimate puts it, and brought into view again once it has been built and measured, so a row taller than guessed does not end up half below the edge. A reveal asked for before the viewport is laid out waits for the first measurement.
    ///
    /// Doesn't subscribe to the scroll offset, for the reason [`ScrollViewport::reveal_rect`] gives.
    pub fn reveal(&self, index: usize, margin: f32) {
        let rows = &self.0;
        if index >= self.len() {
            rows.pending.set(None);
            return;
        }
        rows.pending
            .set((!rows.is_measured(index)).then_some((index, margin)));
        rows.bring_into_view(index, margin);
    }
}

impl Rows {
    fn is_measured(&self, index: usize) -> bool {
        let keys = self.keys.borrow();
        keys.get(index)
            .is_some_and(|key| self.measured.borrow().contains_key(key))
    }

    /// Takes the rows the source now returns, keeping the measurements of the keys still among them.
    fn adopt(&self, keys: Vec<u64>) {
        let heights = {
            let mut measured = self.measured.borrow_mut();
            let mut kept = HashMap::with_capacity(measured.len().min(keys.len()));
            let heights = HeightIndex::new(keys.iter().map(|key| match measured.get(key) {
                Some(&height) => {
                    kept.insert(*key, height);
                    height
                }
                None => self.estimate,
            }));
            *measured = kept;
            heights
        };
        *self.heights.borrow_mut() = heights;
        *self.keys.borrow_mut() = keys;
        self.pending.set(None);
        self.revision.update(|n| *n = n.wrapping_add(1));
    }

    /// Measures the row built for `key` at `at` whenever layout places it.
    fn watch(self: &Rc<Self>, at: usize, key: u64, node: NodeId) {
        let Some(rect) = track_layout(node) else {
            return;
        };
        let rows = Rc::clone(self);
        effect(move || {
            let placed = rect.get();
            if placed.width > 0.0 || placed.height > 0.0 {
                rows.record(at, key, placed.height);
            }
        });
    }

    fn record(self: &Rc<Self>, at: usize, key: u64, height: f32) {
        if self.keys.borrow().get(at) != Some(&key) {
            return;
        }
        self.measured.borrow_mut().insert(key, height);
        let (x, y) = self.viewport.peek_offset();
        let (delta, anchor) = {
            let mut heights = self.heights.borrow_mut();
            let anchor = heights.row_at(y);
            (heights.set(at, height), anchor)
        };
        if delta != 0.0 {
            batch(|| {
                if at < anchor {
                    self.viewport.scroll_to(x, y + delta);
                }
                self.revision.update(|n| *n = n.wrapping_add(1));
            });
        }
        self.follow_pending();
    }

    /// Brings a pending row into view again once the measurements landing now have all reached it.
    fn follow_pending(self: &Rc<Self>) {
        if self.pending.get().is_none() || self.settling.replace(true) {
            return;
        }
        let rows = Rc::clone(self);
        after_settle(move || {
            rows.settling.set(false);
            if let Some((index, margin)) = rows.pending.get() {
                if rows.is_measured(index) {
                    rows.pending.set(None);
                }
                rows.bring_into_view(index, margin);
            }
        });
    }

    fn bring_into_view(&self, index: usize, margin: f32) {
        if self.viewport.rect().peek().height <= 0.0 {
            return;
        }
        let (top, height) = {
            let heights = self.heights.borrow();
            (heights.offset_of(index), heights.height(index))
        };
        let (x, _) = self.viewport.peek_offset();
        self.viewport
            .reveal_rect(Rect::new(x, top, 0.0, height), margin);
    }
}

#[cfg(test)]
#[path = "virtual_list_test.rs"]
mod tests;
