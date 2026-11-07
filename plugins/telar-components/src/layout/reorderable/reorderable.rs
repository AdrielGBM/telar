//! [`reorderable`]: a single list whose items are dragged, or moved with the keyboard, into a different order.

use std::rc::Rc;

use telar::{LayoutError, LayoutItem, Props, Reactive};

use crate::layout::reorder_zones::reorder_zones::{
    DropPreview, ItemBuilder, Reach, ReorderGroup, ReorderZoneProps,
};

/// A list of items the user can drag into a different order.
///
/// The widget owns the *interaction* and nothing else: the items, their identity and what happens to them stay with the caller — [`on_move`](Self::on_move) is called with the two positions and writes nothing itself.
#[derive(Props)]
pub struct ReorderableProps {
    /// How many items there are, read reactively — the list rebuilds when it changes.
    #[props(into, default)]
    pub count: Reactive<usize>,
    /// Builds the widget for the item stored at `index`. Called once per item and reused across a drag, so it may hold state of its own.
    #[props(default = Rc::new(|_| Err(LayoutError::Engine("reorderable: no item builder".into()))))]
    pub item: ItemBuilder,
    /// The drop landed: move the item stored at `from` into slot `to`, counting slots in the list *before* the move. [`telar::apply_move`] is that rule over a `Vec`, if the caller's storage is one.
    #[props(default = Rc::new(|_, _| {}))]
    pub on_move: Rc<dyn Fn(usize, usize)>,
    /// Lay the items out along the horizontal axis. A column otherwise.
    #[props(default)]
    pub row: bool,
    #[props(default)]
    pub gap: f32,
    /// How far the pointer must travel before a press counts as a drag rather than a click on the item.
    #[props(default = 4.0)]
    pub drag_threshold: f32,
    /// How the slot the item in hand would land in is shown.
    #[props(default = DropPreview::Gap)]
    pub drop_preview: DropPreview,
}

/// A list whose items are reordered in place: a [`ReorderGroup`] of one container, whose moves are reported as positions in that container.
///
/// The list is announced as a list and each item as a list item that takes focus. An item is dragged along the list's axis, never past its ends; from the keyboard, Space or Enter picks the focused item up, the arrow keys move it a slot at a time, Space or Enter drops it, and Escape (or focus leaving it) puts it back.
pub fn reorderable(props: ReorderableProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ReorderableProps {
        count,
        item,
        on_move,
        row,
        gap,
        drag_threshold,
        drop_preview,
    } = props;
    ReorderGroup::new()
        .reach(Reach::OwnZone)
        .drop_preview(drop_preview)
        .drag_threshold(drag_threshold)
        .on_move(move |from, to| on_move(from.index, to.index))
        .zone(
            ReorderZoneProps::props()
                .count(count)
                .item(item)
                .row(row)
                .gap(gap)
                .build(),
        )
}

#[cfg(test)]
#[path = "reorderable.test.rs"]
mod tests;
