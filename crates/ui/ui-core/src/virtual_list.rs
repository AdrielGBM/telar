use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use renderer_core::RectStyle;
use std::hash::Hash;

use crate::layout_item::LayoutItem;
use crate::reactive_list::ReactiveList;
use crate::scroll_area::ScrollViewport;
use crate::styled_container::StyledContainer;

/// Which slice of a long list is worth building, given where the viewport currently is.
///
/// Returned as a half-open `[first, last)` over item indices. `overscan` rows are added on each side so a
/// scroll reveals a row that already exists rather than one built during the frame it appears — the difference
/// between a list that scrolls and one that hitches at every boundary.
///
/// A row height of zero (or a viewport not yet laid out) yields the whole range: a virtual list that guessed
/// "nothing is visible" from a missing measurement would render an empty list on its first frame, which reads
/// as a bug rather than as a not-yet-measured layout.
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
    /// The run of skipped rows above or below the window, as a single box of their combined height. Keeping
    /// the scrollable content the full height is what makes the scrollbar and the wheel behave as if every row
    /// were there — which, as far as the user is concerned, they are.
    Gap {
        before: bool,
        height: f32,
    },
    Row(usize, Item),
}

/// A keyed list that builds only the rows currently on screen.
///
/// [`ReactiveList`] builds every item it is given, which is right until the list is long: a wallpaper grid or a
/// full application list pays for thousands of widgets to show a dozen. This builds the visible window plus a
/// little overscan and represents the rest as two spacer boxes, so the content keeps its true height and the
/// scrollbar keeps telling the truth.
///
/// **Fixed row height.** Every row must be `row_height` tall, because the window is computed by division
/// rather than by measurement — measuring rows that have not been built is the circular problem variable-height
/// virtualisation exists to solve, and it needs a size cache and an estimation pass this does not have. A list
/// whose rows genuinely vary belongs in a plain [`ReactiveList`] until that lands.
///
/// `source` still returns every item. That is deliberate: producing a `Vec` of plain data is cheap, and the
/// expensive part — constructing widgets, decoding images, laying out text — is what this defers. A source that
/// is itself expensive should be memoised by the caller, as it would be for any list.
pub struct VirtualList;

impl VirtualList {
    /// `viewport` is the enclosing scroll area's live window (see [`crate::LayoutScrollArea::new_with`]).
    /// `build` constructs one row and receives its index alongside the item, since a virtualised row often
    /// wants to know where it sits.
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

        // A gap keys on its height so a scroll that changes it rebuilds the spacer; a row keys on the caller's own key *and* its index, because the same item at a different index sits at a different height and reusing the node would leave it drawn in the old place.
        let keyer = move |slot: &Slot<Item>| match slot {
            Slot::Gap { before, height } => format!("gap:{before}:{height}"),
            Slot::Row(at, item) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                key(item).hash(&mut hasher);
                format!("row:{at}:{}", std::hash::Hasher::finish(&hasher))
            }
        };

        ReactiveList::with_style(container_style, windowed, keyer, move |slot| match slot {
            Slot::Gap { height, .. } => Ok(Box::new(StyledContainer::new(
                LayoutStyle::new().height(height).flex_shrink(0.0),
                |_: Rect| RectStyle::default(),
                Vec::new(),
            )?) as Box<dyn LayoutItem>),
            Slot::Row(at, item) => build(at, item),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_covers_the_screen_plus_its_overscan() {
        // 20px rows, a 100px window: five rows fit, and the partial sixth is always included.
        let (first, last) = visible_window(0.0, 100.0, 20.0, 1000, 0);
        assert_eq!((first, last), (0, 6));

        // Scrolled to row 10, with two rows of overscan on each side.
        let (first, last) = visible_window(200.0, 100.0, 20.0, 1000, 2);
        assert_eq!((first, last), (8, 18));

        // A partial scroll floors to the row actually on screen rather than rounding past it.
        let (first, _) = visible_window(199.0, 100.0, 20.0, 1000, 0);
        assert_eq!(first, 9, "row 9 is still showing its last pixel");
    }

    #[test]
    fn the_window_is_clamped_at_both_ends() {
        // At the very top, overscan cannot go negative.
        assert_eq!(visible_window(0.0, 100.0, 20.0, 1000, 5), (0, 11));
        // At the bottom, it cannot run past the list.
        assert_eq!(visible_window(19_800.0, 100.0, 20.0, 1000, 5), (985, 1000));
        // A scroll offset past the end (a list that shrank under the viewport) still yields a valid range.
        let (first, last) = visible_window(100_000.0, 100.0, 20.0, 10, 0);
        assert!(first <= last && last <= 10, "got {first}..{last}");
    }

    #[test]
    fn an_unmeasured_viewport_renders_everything_rather_than_nothing() {
        // The first frame, before layout has given the scroll area a height: showing nothing would look like a broken list, and showing everything is exactly what a plain ReactiveList would have done.
        assert_eq!(visible_window(0.0, 0.0, 20.0, 40, 0), (0, 40));
        assert_eq!(visible_window(0.0, 100.0, 0.0, 40, 0), (0, 40));
        assert_eq!(
            visible_window(0.0, 100.0, 20.0, 0, 0),
            (0, 0),
            "an empty list is empty"
        );
    }
}
