//! [`VirtualList`]: a reactive list that builds only the rows inside its viewport, plus an overscan band.

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::{RwSignal, after_settle, batch, effect, signal};
use renderer_core::RectStyle;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
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

/// How far, as a fraction of the current estimate, the mean of the measured rows has to move before the unbuilt rows are re-estimated.
const ESTIMATE_TOLERANCE: f32 = 0.01;

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
/// - [`measured`](Self::measured) — **variable row height**. Rows that have never been built stand at an estimate, which starts as the caller's and from then on is the mean height of the rows measured so far, so the scrollbar settles on the list's real length instead of drifting by the same error for every row. Each built row is measured once layout has placed it, and the measurement replaces the estimate in a [`HeightIndex`], so finding the rows on screen stays `O(log n)` however long the list. While the estimates are corrected, by a measurement or by the estimate itself moving, the first row on screen holds still: the scroll offset moves by however much the rows above it turned out taller or shorter than guessed. Bringing a row into view goes through the list's [`VirtualRows`] handle, which knows where unbuilt rows stand.
///
/// `source` still returns every item, but runs only when what it reads changes: either constructor keeps the list it last returned, and a scroll or a moved cursor cuts the new window from that, cloning just the rows it builds — which is why the items must be [`VirtualItem`], any `Clone` type. Producing a `Vec` of plain data is cheap next to what this defers: constructing widgets, decoding images, laying out text.
pub struct VirtualList;

impl VirtualList {
    /// A list whose every row is `row_height` tall, building the rows on screen and `overscan` more on each side: until the viewport is laid out, the rows a surface's height of it would show, and every row only where the surface's size is not known either.
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
        Item: VirtualItem,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(usize, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        let items = stored_items(source, |_| {});
        let (_, offset_y) = viewport.offset();
        let rect = viewport.rect();
        let windowed = move || {
            let offset = offset_y.get();
            let viewport_height = laid_out_or_surface(rect.get().height);
            items.with(|items| {
                let count = items.len();
                let (first, last) =
                    visible_window(offset, viewport_height, row_height, count, overscan);
                let mut slots = Vec::with_capacity(last - first + 2);
                if first > 0 {
                    slots.push(Slot::Gap {
                        before: true,
                        height: first as f32 * row_height,
                    });
                }
                slots.extend((first..last).map(|at| Slot::Row(at, items[at].clone())));
                if last < count {
                    slots.push(Slot::Gap {
                        before: false,
                        height: (count - last) as f32 * row_height,
                    });
                }
                slots
            })
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
    /// Until the viewport is laid out it builds a surface's height of rows rather than every row, as [`new`](Self::new) does, and only where the surface's size is not known either does it build them all.
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
        Item: VirtualItem,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(usize, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        let rows = rows.0;
        let items = {
            let rows = Rc::clone(&rows);
            stored_items(source, move |fresh: &[Item]| {
                rows.adopt(fresh.iter().map(|item| hash_key(&key(item))).collect());
            })
        };

        let (_, offset_y) = rows.viewport.offset();
        let rect = rows.viewport.rect();
        let windowed = {
            let rows = Rc::clone(&rows);
            move || {
                rows.revision.get();
                let offset = offset_y.get();
                let viewport_height = laid_out_or_surface(rect.get().height);
                items.with(|items| {
                    let heights = rows.heights.borrow();
                    let keys = rows.keys.borrow();
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
                    slots.extend(
                        (first..last).map(|at| Slot::Row(at, (keys[at], items[at].clone()))),
                    );
                    if last < heights.len() {
                        slots.push(Slot::Gap {
                            before: false,
                            height: heights.span(last, heights.len()),
                        });
                    }
                    slots
                })
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

/// What [`VirtualList`] asks of its items: the list the source last returned is kept and each row is built from a clone of its item, so the source runs only when what it reads changes rather than on every scroll. A trait of its own so that a type without `Clone` is told what the list needed it for.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be the item of a `virtual` list: its rows are built from clones of the items, so it must implement `Clone`",
    label = "add `#[derive(Clone)]` to this type, or iterate a `Clone` projection of it"
)]
pub trait VirtualItem: Clone + 'static {}

impl<Item: Clone + 'static> VirtualItem for Item {}

/// The items `source` returns, taken again whenever what it reads changes and handed to `adopt` before the list sees them, so a window moved by a scroll or a cursor is cut from them without running the source.
fn stored_items<Item: 'static>(
    source: impl Fn() -> Vec<Item> + 'static,
    adopt: impl Fn(&[Item]) + 'static,
) -> RwSignal<Vec<Item>> {
    let items = signal(Vec::new());
    effect(move || {
        let fresh = source();
        adopt(&fresh);
        items.set(fresh);
    });
    items
}

/// The height of a viewport laid out at `height`, or the surface's until it is: nothing taller than the surface can be on screen at once, and building a hundred thousand rows on the first frame to learn which of them are would cost what virtualising saves.
fn laid_out_or_surface(height: f32) -> f32 {
    match height {
        laid_out if laid_out > 0.0 => laid_out,
        _ => surface_size().height,
    }
}

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
/// A clone is the same handle. Reads answer what is known now — measured heights for rows that have been built, the running estimate for the rest — without subscribing, so they are safe to call from the effect that follows a selection; the positions they give move as rows are measured.
#[derive(Clone)]
pub struct VirtualRows(Rc<Rows>);

struct Rows {
    viewport: ScrollViewport,
    seed: f32,
    estimate: Cell<f32>,
    header: Cell<f32>,
    heights: RefCell<HeightIndex>,
    keys: RefCell<Vec<u64>>,
    measured: RefCell<HashMap<u64, f32>>,
    measured_total: Cell<f64>,
    /// Bumped whenever a height changes, so the window is worked out again.
    revision: RwSignal<u64>,
    /// A row asked to be brought into view before it had been measured, and the margin it asked for.
    pending: Cell<Option<(usize, f32)>>,
    settling: Cell<bool>,
    estimating: Cell<bool>,
}

impl VirtualRows {
    /// Rows scrolled by `viewport`, the enclosing scroll area's live window (see [`crate::LayoutScrollArea::new_with`]), each standing `estimate` px tall until it is built and measured.
    ///
    /// `estimate` is where the unbuilt rows start: once rows have been measured they stand at the mean of those instead, so the closer it is to a typical row, the less the scrollbar moves while the first screenfuls are measured. The first row on screen holds still either way.
    pub fn new(viewport: ScrollViewport, estimate: f32) -> Self {
        Self(Rc::new(Rows {
            viewport,
            seed: estimate.max(0.0),
            estimate: Cell::new(estimate.max(0.0)),
            header: Cell::new(0.0),
            heights: RefCell::default(),
            keys: RefCell::default(),
            measured: RefCell::default(),
            measured_total: Cell::new(0.0),
            revision: signal(0),
            pending: Cell::new(None),
            settling: Cell::new(false),
            estimating: Cell::new(false),
        }))
    }

    /// Rows that stand under a header `height` px tall at the top of the scroll's content, held at the top of the view while they scroll under it: a table's column headings. Where a row stands counts the header, and the rows on screen and a row brought into view are the ones it leaves uncovered.
    pub fn under_header(self, height: f32) -> Self {
        self.0.header.set(height.max(0.0));
        self
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
        self.0.header.get() + self.0.heights.borrow().offset_of(index)
    }

    /// How tall row `index` is: measured once it has been built, the current estimate before.
    pub fn height_of(&self, index: usize) -> f32 {
        self.0.heights.borrow().height(index)
    }

    /// The row standing at `offset` in the scroll's content, the first row for an offset in the header.
    pub fn row_at(&self, offset: f32) -> usize {
        self.0.heights.borrow().row_at(offset - self.0.header.get())
    }

    /// The height of every row together: the content the scroll scrolls, less the header.
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

    /// Takes the keys of the rows the source now returns, keeping the measurements of the keys still among them. When the change is an edit around a few rows, only those rows are looked up and the index is spliced; when most rows changed, it is built over again. The items arriving after them are what works the window out again.
    fn adopt(&self, keys: Vec<u64>) {
        let spliced = {
            let old = self.keys.borrow();
            self.splice_changes(&old, &keys)
        };
        if !spliced {
            self.rebuild(&keys);
        }
        *self.keys.borrow_mut() = keys;
        self.pending.set(None);
    }

    fn splice_changes(&self, old: &[u64], new: &[u64]) -> bool {
        let shared = old.len().min(new.len());
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take(shared - prefix)
            .take_while(|(a, b)| a == b)
            .count();
        let removed = &old[prefix..old.len() - suffix];
        let added = &new[prefix..new.len() - suffix];
        if removed.len().max(added.len()) * 2 > old.len().max(new.len()) {
            return false;
        }
        let estimate = self.estimate.get();
        let mut measured = self.measured.borrow_mut();
        let still_there: HashSet<u64> = added.iter().copied().collect();
        for key in removed.iter().filter(|key| !still_there.contains(key)) {
            if let Some(height) = measured.remove(key) {
                self.measured_total
                    .set(self.measured_total.get() - f64::from(height));
            }
        }
        let heights = added
            .iter()
            .map(|key| measured.get(key).copied().unwrap_or(estimate));
        self.heights
            .borrow_mut()
            .splice(prefix, removed.len(), heights);
        true
    }

    fn rebuild(&self, keys: &[u64]) {
        let mut measured = self.measured.borrow_mut();
        let mut kept = HashMap::with_capacity(measured.len().min(keys.len()));
        let mut total = 0.0f64;
        for key in keys {
            if let Some(&height) = measured.get(key)
                && kept.insert(*key, height).is_none()
            {
                total += f64::from(height);
            }
        }
        *measured = kept;
        self.measured_total.set(total);
        let estimate = self.mean_height(measured.len()).round();
        self.estimate.set(estimate);
        *self.heights.borrow_mut() = HeightIndex::new(
            keys.iter()
                .map(|key| measured.get(key).copied().unwrap_or(estimate)),
        );
    }

    fn mean_height(&self, count: usize) -> f32 {
        match count {
            0 => self.seed,
            _ => (self.measured_total.get() / count as f64) as f32,
        }
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
                let height = layout_reactive::unrounded_height(node).unwrap_or(placed.height);
                rows.record(at, key, height);
            }
        });
    }

    fn record(self: &Rc<Self>, at: usize, key: u64, height: f32) {
        if self.keys.borrow().get(at) != Some(&key) {
            return;
        }
        let previous = self.measured.borrow_mut().insert(key, height);
        self.measured_total.set(
            self.measured_total.get() + f64::from(height) - f64::from(previous.unwrap_or(0.0)),
        );
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
        self.follow_estimate();
    }

    /// Re-estimates the unbuilt rows once the measurements landing now have all been taken, so a screenful of rows measured together moves the scrollbar once rather than once each.
    fn follow_estimate(self: &Rc<Self>) {
        if self.estimating.replace(true) {
            return;
        }
        let rows = Rc::clone(self);
        after_settle(move || {
            rows.estimating.set(false);
            rows.retarget();
        });
    }

    /// Stands every unmeasured row at the mean of the measured ones to the whole pixel, which is where layout puts a gap so the index and the layout keep agreeing, moving the offset by however far that moved the rows above the first one on screen, so that row stays put. A mean that has moved less than [`ESTIMATE_TOLERANCE`] (and a pixel) from the estimate is left, which is what keeps measuring the rows this brings on screen from nudging the estimate back and forth.
    fn retarget(&self) {
        let mean = self.mean_height(self.measured.borrow().len());
        let current = self.estimate.get();
        if (mean - current).abs() < (current * ESTIMATE_TOLERANCE).max(1.0) {
            return;
        }
        let target = mean.round();
        self.estimate.set(target);
        let (x, y) = self.viewport.peek_offset();
        let (anchor, before) = {
            let heights = self.heights.borrow();
            let anchor = heights.row_at(y);
            (anchor, heights.offset_of(anchor))
        };
        let after = {
            let measured = self.measured.borrow();
            let keys = self.keys.borrow();
            let mut heights = self.heights.borrow_mut();
            *heights = HeightIndex::new(
                keys.iter()
                    .map(|key| measured.get(key).copied().unwrap_or(target)),
            );
            heights.offset_of(anchor)
        };
        batch(|| {
            if after != before {
                self.viewport.scroll_to(x, y + after - before);
            }
            self.revision.update(|n| *n = n.wrapping_add(1));
        });
        if let Some((index, margin)) = self.pending.get() {
            self.bring_into_view(index, margin);
        }
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
        let (x, y) = self.viewport.peek_offset();
        if let Some(furthest) = self.viewport.max_offset_y()
            && y > furthest
        {
            self.viewport.scroll_to(x, furthest);
        }
        self.viewport
            .reveal_rect(Rect::new(x, top, 0.0, height + self.header.get()), margin);
    }
}

#[cfg(test)]
#[path = "virtual_list_test.rs"]
mod tests;
