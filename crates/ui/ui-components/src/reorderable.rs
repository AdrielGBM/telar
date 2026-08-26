use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use ui_core::{
    Axis, LayoutItem, ReactiveList, StyledContainer, box_item, insertion_index, track_layout,
};

use crate::shared::props_default;

/// One item's widget, built on demand from its position in the caller's storage.
pub type ItemBuilder = Box<dyn Fn(usize) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// What a live drag has established: which item was picked up, and which slot it currently sits over.
///
/// `rects` is frozen at the press. The strip opens a gap as the pointer moves, so reading the rects live
/// would feed that back into itself — the gap shifts the items, which moves the slot, which shifts the gap —
/// and the target would oscillate between two positions instead of settling.
#[derive(Clone)]
struct Drag {
    from: usize,
    to: usize,
    rects: Rc<Vec<Rect>>,
}

/// A strip of items the user can drag into a different order.
///
/// The widget owns the *interaction* and nothing else: which slot the pointer is over, showing a gap there,
/// and reporting the move once. The items, their identity and what happens to them stay with the caller —
/// [`on_move`](Self::on_move) is called with the two positions and writes nothing itself, so a strip backed
/// by a config file, a plugin host or three separate zones all use the same widget.
pub struct ReorderableProps {
    /// How many items there are, read reactively — the strip rebuilds when it changes.
    pub count: Box<dyn Fn() -> usize>,
    /// Builds the widget for the item stored at `index`. Called once per item and reused across a drag, so it
    /// may hold state of its own.
    pub item: ItemBuilder,
    /// The drop landed: move the item stored at `from` into slot `to`, counting slots in the list *before*
    /// the move. [`ui_core::apply_move`] is that rule over a `Vec`, if the caller's storage is one.
    pub on_move: Box<dyn Fn(usize, usize)>,
    /// Lay the strip out along the horizontal axis. A column otherwise, matching `ReactiveList`'s own default.
    pub row: bool,
    pub gap: f32,
    /// How far the pointer must travel before a press counts as a drag rather than a click on the item.
    pub drag_threshold: f32,
}

props_default!(ReorderableProps {
    count: (Box::new(|| 0)),
    item: (Box::new(|_| Err(LayoutError::Engine("reorderable: no item builder".into())))),
    on_move: (Box::new(|_, _| {})),
    row: zero,
    gap: zero,
    drag_threshold: (4.0),
});

pub fn reorderable(props: ReorderableProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ReorderableProps {
        count,
        item,
        on_move,
        row,
        gap,
        drag_threshold,
    } = props;

    let axis = if row {
        Axis::Horizontal
    } else {
        Axis::Vertical
    };
    let drag: RwSignal<Option<Drag>> = signal(None);
    let rects: Rc<RefCell<Vec<Option<RwSignal<Rect>>>>> = Rc::new(RefCell::new(Vec::new()));
    let on_move = Rc::new(on_move);

    let count = Rc::new(count);
    let source_count = Rc::clone(&count);
    let source = move || (0..source_count()).collect::<Vec<usize>>();

    let build_rects = Rc::clone(&rects);
    let build_drag = drag;
    let build_move = Rc::clone(&on_move);
    let build_count = Rc::clone(&count);
    let build = move |index: usize| -> Result<Box<dyn LayoutItem>, LayoutError> {
        let child = item(index)?;
        let node = child.layout_node();
        let rect = track_layout(node).ok_or_else(|| {
            LayoutError::Engine("reorderable: an item's layout node is not in the live tree".into())
        })?;
        remember(&build_rects, index, rect);

        let move_rects = Rc::clone(&build_rects);
        let move_drag = build_drag;
        let move_rect = rect;
        let end_drag = build_drag;
        let end_move = Rc::clone(&build_move);
        let gap_drag = build_drag;
        let gap_count = Rc::clone(&build_count);
        let slot_style = move || slot_style(index, gap_count(), gap_drag.get().as_ref(), axis);
        // The drag signal is read only by `styled_by`'s own effect, never here and never by the list's
        // `source`: `build` runs inside the reconcile effect, so a read at either of those places would
        // subscribe *that* effect to the drag and make the next pointer move reconcile the list from inside
        // its own event dispatch. Restyling one node touches the list not at all, which is why the gap is a
        // layout style rather than a reordered source.
        let wrapper = StyledContainer::new(
            LayoutStyle::new(),
            |_| Default::default(),
            vec![box_item(child)],
        )?
        .styled_by(slot_style)
        .drag_threshold(drag_threshold)
        // `on_drag` reports widget-local coordinates; the strip's slots are in surface coordinates, so the
        // item's own origin is what converts between them.
        .on_drag(move |x, y| {
            let origin = move_rect.peek();
            let point = (origin.x + x, origin.y + y);
            let frozen = match move_drag.peek() {
                Some(existing) => existing.rects,
                None => Rc::new(snapshot(&move_rects)),
            };
            let to = insertion_index(&frozen, point, axis);
            move_drag.set(Some(Drag {
                from: index,
                to,
                rects: frozen,
            }));
        })
        .on_drag_end(move |_x, _y| {
            let Some(landed) = end_drag.peek() else {
                return;
            };
            end_drag.set(None);
            if landed.to != landed.from && landed.to != landed.from + 1 {
                end_move(landed.from, landed.to);
            }
        });
        Ok(box_item(wrapper))
    };

    let list = ReactiveList::new(source, |index: &usize| *index, build, gap)?;
    Ok(box_item(if row { list.as_row() } else { list }))
}

/// The layout style for the item stored at `index`: a leading gap the width of the dragged item when the
/// drop would land in front of it, and a trailing one for the last item when the drop lands past everything.
///
/// The gap is the dragged item's own extent, so the strip opens by exactly as much as the drop will consume
/// and nothing else shifts when the drag ends.
fn slot_style(index: usize, len: usize, drag: Option<&Drag>, axis: Axis) -> LayoutStyle {
    let base = LayoutStyle::new();
    let Some(drag) = drag else { return base };
    let size = drag
        .rects
        .get(drag.from)
        .map(|rect| match axis {
            Axis::Horizontal => rect.width,
            Axis::Vertical => rect.height,
        })
        .unwrap_or(0.0);
    let leading = drag.to == index;
    let trailing = drag.to >= len && index + 1 == len;
    match (leading, trailing, axis) {
        (true, _, Axis::Horizontal) => base.margin_inline_start(size),
        (true, _, Axis::Vertical) => base.margin_block_start(size),
        (_, true, Axis::Horizontal) => base.margin_inline_end(size),
        (_, true, Axis::Vertical) => base.margin_block_end(size),
        _ => base,
    }
}

fn remember(rects: &Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>, index: usize, rect: RwSignal<Rect>) {
    let mut slots = rects.borrow_mut();
    if slots.len() <= index {
        slots.resize(index + 1, None);
    }
    slots[index] = Some(rect);
}

/// The items' rects in *display* order, which for a strip that is not mid-drag is stored order.
fn snapshot(rects: &Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>) -> Vec<Rect> {
    rects
        .borrow()
        .iter()
        .map(|slot| slot.as_ref().map(|r| r.peek()).unwrap_or_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use layout_core::LayoutStyle;
    use reactive_core::{RwSignal, signal};
    use renderer_core::RectStyle;
    use ui_core::{Component, LayoutItem, StyledContainer, box_item};

    use super::*;
    use crate::harness::{lay_out_row, moved, press, release};
    use crate::test_support::fresh_layout_runtime;

    const ITEM: f32 = 100.0;

    type Strip = (
        Box<dyn LayoutItem>,
        RwSignal<Vec<char>>,
        Rc<RefCell<Vec<(usize, usize)>>>,
    );

    /// A strip of four fixed-width pills over a caller-owned `Vec`, plus the moves it was told to make.
    fn strip() -> Strip {
        let items = signal(vec!['a', 'b', 'c', 'd']);
        let moves = Rc::new(RefCell::new(Vec::new()));

        let count_items = items;
        let recorded = Rc::clone(&moves);
        let moved_items = items;
        let widget = reorderable(ReorderableProps {
            count: Box::new(move || count_items.get().len()),
            item: Box::new(|_| {
                Ok(box_item(StyledContainer::new(
                    LayoutStyle::new().width(ITEM).height(40.0),
                    |_| RectStyle::default(),
                    vec![],
                )?))
            }),
            on_move: Box::new(move |from, to| {
                recorded.borrow_mut().push((from, to));
                let mut current = moved_items.peek();
                ui_core::apply_move(&mut current, from, to);
                moved_items.set(current);
            }),
            row: true,
            gap: 0.0,
            drag_threshold: 0.0,
        })
        .unwrap();
        (widget, items, moves)
    }

    /// Dragging the first pill past the centre of the third must land it there, and must report the slot in
    /// the frame of reference `apply_move` reads — which is what makes the two halves agree.
    #[test]
    fn a_pill_dragged_past_two_centres_lands_after_them() {
        fresh_layout_runtime();
        let (mut widget, items, moves) = strip();
        lay_out_row(widget.layout_node(), 400.0, 40.0);

        widget.on_event(&press(10.0, 20.0));
        widget.on_event(&moved(260.0, 20.0));
        widget.on_event(&release(260.0, 20.0));

        assert_eq!(*moves.borrow(), vec![(0, 3)]);
        assert_eq!(items.peek(), vec!['b', 'c', 'a', 'd']);
    }

    /// A press that never left the pill is a click on it, not a reorder — the case that makes a strip of
    /// buttons still usable as buttons.
    #[test]
    fn a_press_that_does_not_travel_moves_nothing() {
        fresh_layout_runtime();
        let (mut widget, items, moves) = strip();
        lay_out_row(widget.layout_node(), 400.0, 40.0);

        widget.on_event(&press(10.0, 20.0));
        widget.on_event(&release(12.0, 20.0));

        assert!(moves.borrow().is_empty(), "{:?}", moves.borrow());
        assert_eq!(items.peek(), vec!['a', 'b', 'c', 'd']);
    }

    /// Dropping an item back over its own slot is not a move. Reporting one would rewrite the caller's list
    /// (and every signal reading it) for a gesture that changed nothing.
    #[test]
    fn dropping_a_pill_where_it_started_reports_nothing() {
        fresh_layout_runtime();
        let (mut widget, _items, moves) = strip();
        lay_out_row(widget.layout_node(), 400.0, 40.0);

        widget.on_event(&press(10.0, 20.0));
        widget.on_event(&moved(40.0, 20.0));
        widget.on_event(&release(40.0, 20.0));

        assert!(moves.borrow().is_empty(), "{:?}", moves.borrow());
    }
}
