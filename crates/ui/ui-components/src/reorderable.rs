use std::cell::RefCell;
use std::rc::Rc;
use telar_macros::Props;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal, signal};
use ui_core::{
    Axis, LayoutItem, ReactiveList, StyledContainer, box_item, insertion_index, track_layout,
};

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
    /// Where the stroke was first reported, and where it is now. The difference is what the picked-up item is
    /// drawn by, so it travels with the pointer instead of sitting in the slot it is being taken out of.
    from_point: (f32, f32),
    at: (f32, f32),
}

/// A strip of items the user can drag into a different order.
///
/// The widget owns the *interaction* and nothing else: which slot the pointer is over, showing a gap there,
/// and reporting the move once. The items, their identity and what happens to them stay with the caller —
/// [`on_move`](Self::on_move) is called with the two positions and writes nothing itself, so a strip backed
/// by a config file, a plugin host or three separate zones all use the same widget.
#[derive(Props)]
pub struct ReorderableProps {
    /// How many items there are, read reactively — the strip rebuilds when it changes.
    #[props(into, default)]
    pub count: Reactive<usize>,
    /// Builds the widget for the item stored at `index`. Called once per item and reused across a drag, so it
    /// may hold state of its own.
    #[props(default = Box::new(|_| Err(LayoutError::Engine("reorderable: no item builder".into()))))]
    pub item: ItemBuilder,
    /// The drop landed: move the item stored at `from` into slot `to`, counting slots in the list *before*
    /// the move. [`ui_core::apply_move`] is that rule over a `Vec`, if the caller's storage is one.
    #[props(default = Box::new(|_, _| {}))]
    pub on_move: Box<dyn Fn(usize, usize)>,
    /// Lay the strip out along the horizontal axis. A column otherwise, matching `ReactiveList`'s own default.
    #[props(default)]
    pub row: bool,
    #[props(default)]
    pub gap: f32,
    /// How far the pointer must travel before a press counts as a drag rather than a click on the item.
    #[props(default = 4.0)]
    pub drag_threshold: f32,
}

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

    let source_count = count.clone();
    let source = move || (0..source_count.get()).collect::<Vec<usize>>();

    let build_rects = rects.clone();
    let build_drag = drag;
    let build_move = on_move.clone();
    let build_count = count.clone();
    let build = move |index: usize| -> Result<Box<dyn LayoutItem>, LayoutError> {
        let child = item(index)?;
        let node = child.layout_node();
        let rect = track_layout(node).ok_or_else(|| {
            LayoutError::Engine("reorderable: an item's layout node is not in the live tree".into())
        })?;
        remember(&build_rects, index, rect);

        let move_rects = build_rects.clone();
        let move_drag = build_drag;
        let move_rect = rect;
        let end_drag = build_drag;
        let end_move = build_move.clone();
        let gap_drag = build_drag;
        let gap_count = build_count.clone();
        let bounds_rects = build_rects.clone();
        let bounds_rect = rect;
        let bounds_drag = build_drag;
        let slot_style =
            move || slot_style(index, gap_count.get(), gap_drag.get().as_ref(), axis, gap);
        // The drag signal is read only by `styled_by`'s own effect, never here and never by the list's
        // `source`: `build` runs inside the reconcile effect, so a read at either of those places would
        // subscribe *that* effect to the drag and make the next pointer move reconcile the list from inside
        // its own event dispatch. Restyling one node touches the list not at all, which is why the gap is a
        // layout style rather than a reordered source.
        let carried = build_drag;
        let wrapper = StyledContainer::new(
            LayoutStyle::new(),
            |_| Default::default(),
            vec![box_item(child)],
        )?
        .styled_by(slot_style)
        // **The one being dragged goes with the pointer.** The gap says where it will land; without this nothing says what is being put there — the item sits in the slot it is leaving, and a strip with a hole opening beside an item that has not moved reads as two of it.
        .with_transform(move |rect| {
            let held = carried.get()?;
            if held.from != index {
                return None;
            }
            // Measured from where it *was*, not from where it is. The gap opening at the drop moves every item after it — this one included, since it is still in the flow — so a translate taken from the laid-out place carries the gap as well as the pointer: dragged leftwards, the item jumped a whole slot to the right the moment the gap opened in front of it.
            let was = held.rects.get(index).copied().unwrap_or(rect);
            let (dx, dy) = (
                held.at.0 - held.from_point.0 + was.x - rect.x,
                held.at.1 - held.from_point.1 + was.y - rect.y,
            );
            Some([1.0, 0.0, 0.0, 1.0, dx, dy])
        })
        .drag_threshold(drag_threshold)
        // **A strip is reordered along itself.** The other coordinate says nothing about where an item lands, so carrying it only lets the one in hand wander off the line the strip lives on.
        .drag_axis(match axis {
            Axis::Horizontal => ui_core::DragAxis::Horizontal,
            Axis::Vertical => ui_core::DragAxis::Vertical,
        })
        // And no further than the strip: a pointer dragged out of the window goes on reporting, and an item that followed it there is one nobody can see to drop.
        .drag_within(move || carried_within(bounds_drag, &bounds_rects, bounds_rect))
        // `on_drag` reports widget-local coordinates; the strip's slots are in surface coordinates, so the
        // item's own origin is what converts between them.
        .on_drag(move |x, y| {
            let origin = move_rect.peek();
            let point = (origin.x + x, origin.y + y);
            let (frozen, from_point) = match move_drag.peek() {
                Some(existing) => (existing.rects, existing.from_point),
                // The first report is the reference the item is carried from: the press point itself is not reported, and starting from it would jump the item by the threshold the moment it moved.
                None => (Rc::new(snapshot(&move_rects)), point),
            };
            let to = insertion_index(&frozen, point, axis);
            move_drag.set(Some(Drag {
                from: index,
                to,
                rects: frozen,
                from_point,
                at: point,
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

/// The layout style for the item stored at `index` while a stroke is running: the slot the carried item came
/// out of pulled shut, and one of its own size opened where the drop would land.
///
/// **The two are added, never chosen between.** The item that is being carried can also be the one the gap
/// belongs to — the last of a strip, dragged past the end — and picking one of the two rules let the trailing
/// gap overwrite the closing of the slot, so that one item opened two holes and neither had anything in it.
///
/// Both are the item's extent *and* the spacing the strip puts between its items, which is what makes them
/// cancel exactly: a strip mid-drag is the width it was, wherever the drop is heading.
fn slot_style(index: usize, len: usize, drag: Option<&Drag>, axis: Axis, gap: f32) -> LayoutStyle {
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
    let opening = size + gap;
    let start = match drag.to == index {
        true => opening,
        false => 0.0,
    };
    let carried = match drag.from == index {
        true => -opening,
        false => 0.0,
    };
    let past_the_end = match drag.to >= len && index + 1 == len {
        true => opening,
        false => 0.0,
    };
    let end = carried + past_the_end;
    match axis {
        Axis::Horizontal => base.margin_inline_start(start).margin_inline_end(end),
        Axis::Vertical => base.margin_block_start(start).margin_block_end(end),
    }
}

/// How far the item at `mine` may be carried: the strip's own extent, said in that item's coordinates.
///
/// The strip is the union of what it is showing rather than a rect of its own — the widget lays its items out
/// and never asks for a box around them, and the union is the same answer without one.
///
/// **Frozen once a stroke is running**, for the reason [`Drag`] freezes its rects: the gap moves the items, so
/// bounds read live grow with it, which moves the clamped point, which moves the gap. At the edge — the only
/// place the clamp binds — that loop had the item swapping with its neighbour for ever on a pointer that was
/// standing still.
fn carried_within(
    drag: RwSignal<Option<Drag>>,
    rects: &Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>,
    mine: RwSignal<Rect>,
) -> Rect {
    let held = match drag.peek() {
        Some(running) => running.rects,
        None => Rc::new(snapshot(rects)),
    };
    let strip = held
        .iter()
        .copied()
        .filter(|rect| rect.width > 0.0 || rect.height > 0.0)
        .reduce(|held, rect| held.union(rect))
        .unwrap_or_default();
    let me = mine.peek();
    Rect::new(strip.x - me.x, strip.y - me.y, strip.width, strip.height)
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
    use crate::harness::{lay_out_row, moved, press, release, route};
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
        let recorded = moves.clone();
        let moved_items = items;
        let widget = reorderable(
            ReorderableProps::props()
                .count(Reactive::of(move || count_items.get().len()))
                .item(Box::new(|_| {
                    Ok(box_item(StyledContainer::new(
                        LayoutStyle::new().width(ITEM).height(40.0),
                        |_| RectStyle::default(),
                        vec![],
                    )?))
                }))
                .on_move(Box::new(move |from, to| {
                    recorded.borrow_mut().push((from, to));
                    let mut current = moved_items.peek();
                    ui_core::apply_move(&mut current, from, to);
                    moved_items.set(current);
                }))
                .row(true)
                .gap(0.0)
                .drag_threshold(0.0)
                .build(),
        )
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

    /// Where each pill is actually drawn, which for one being carried is not where it was laid out.
    fn drawn_at(tree: &ui_core::ComponentList) -> Vec<f32> {
        let mut found = Vec::new();
        renderer_core::for_each_with_matrix(&tree.commands(), |command, matrix| {
            if let renderer_core::DrawCommand::Rect { rect, .. } = command
                && rect.width == ITEM
            {
                found.push(matrix[0] * rect.x + matrix[2] * rect.y + matrix[4]);
            }
        });
        found
    }

    /// **The one being dragged goes with the pointer.** The gap says where it will land; nothing said what
    /// was going into it, so the strip opened a hole beside a pill that had not moved — which reads as two of
    /// the same pill rather than as one being carried.
    #[test]
    fn the_pill_being_dragged_travels_with_the_pointer() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);
        let resting = drawn_at(&tree);

        route(&mut tree, &press(10.0, 20.0));
        route(&mut tree, &moved(60.0, 20.0));
        route(&mut tree, &moved(160.0, 20.0));

        // Exactly what the pointer travelled from where it was pressed: 10 → 160.
        let carried = drawn_at(&tree);
        assert_eq!(
            carried[0] - resting[0],
            150.0,
            "la pastilla no siguió al puntero: {resting:?} → {carried:?}"
        );

        route(&mut tree, &release(160.0, 20.0));
        let landed = drawn_at(&tree);
        assert!(
            landed.iter().all(|x| resting.contains(x)),
            "quedó una pastilla fuera de su hueco: {landed:?}"
        );
    }

    /// **One hole, not two.** The pill travels with the pointer, so the slot it came out of has to close
    /// behind it: leaving it open showed a hole where it was *and* the gap where it is going, with nothing in
    /// either — which reads as the strip having lost one.
    #[test]
    fn the_slot_the_pill_came_out_of_closes_behind_it() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);
        let resting = drawn_at(&tree);

        route(&mut tree, &press(10.0, 20.0));
        route(&mut tree, &moved(260.0, 20.0));
        ui_core::relayout_if_dirty();

        // The three that stayed: each has come forward by one slot, and the gap has opened where the drop lands.
        let carried = drawn_at(&tree);
        assert_eq!(
            carried[2] - resting[2],
            -ITEM,
            "el hueco de origen no se cerró: {resting:?} → {carried:?}"
        );
    }

    /// **A strip mid-drag is the width it was.** The slot the carried item came out of closes by exactly what
    /// the gap it is heading for opens, so the strip never grows a second hole — which is what the last item
    /// dragged past the end did: it was both the one being carried and the one the trailing gap belongs to,
    /// and the two rules overwrote each other instead of adding up.
    #[test]
    fn the_strip_is_the_width_it_was_wherever_the_drop_is_heading() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let strip_rect = ui_core::track_layout(node).unwrap();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);
        let resting = strip_rect.get().width;

        // The last pill, taken past the end — the case where one item is both carried and trailing.
        route(&mut tree, &press(350.0, 20.0));
        for at in [360.0, 390.0, 4000.0, 360.0] {
            route(&mut tree, &moved(at, 20.0));
            ui_core::relayout_if_dirty();
            assert_eq!(
                strip_rect.get().width,
                resting,
                "la tira creció un hueco de más con el puntero en {at}"
            );
        }
    }

    /// **Leftwards too.** The gap opens in front of the one in hand and moves it along with everything else
    /// after it, so a translate measured from where it is laid out carried the gap as well as the pointer:
    /// dragging left, the pill jumped a whole slot to the right the instant the gap appeared.
    #[test]
    fn a_pill_dragged_leftwards_follows_the_pointer_and_not_the_gap() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);
        let resting = drawn_at(&tree);

        // The third pill, taken to the left past the first one's centre.
        route(&mut tree, &press(250.0, 20.0));
        route(&mut tree, &moved(40.0, 20.0));
        ui_core::relayout_if_dirty();

        let carried = drawn_at(&tree);
        assert_eq!(
            carried[4] - resting[4],
            -210.0,
            "la pastilla no siguió al puntero hacia la izquierda: {resting:?} → {carried:?}"
        );
    }

    /// **And it stands still where the pointer does.** The bounds were read off the live rects, which the gap
    /// moves — so at the edge, where the clamp is what decides the point, the strip fed its own gap back into
    /// the slot it was computing and swapped the pill with its neighbour for ever.
    #[test]
    fn a_pill_held_against_the_edge_stays_where_it_is_put() {
        fresh_layout_runtime();
        let (widget, _items, moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);

        route(&mut tree, &press(10.0, 20.0));
        route(&mut tree, &moved(4000.0, 20.0));
        ui_core::relayout_if_dirty();
        let against = drawn_at(&tree);

        // The same place, again and again: nothing about the strip may change while the pointer does not.
        for _ in 0..4 {
            route(&mut tree, &moved(4000.0, 20.0));
            ui_core::relayout_if_dirty();
            assert_eq!(
                drawn_at(&tree),
                against,
                "la tira se mueve sola con el puntero quieto en el borde"
            );
        }

        route(&mut tree, &release(4000.0, 20.0));
        assert_eq!(moves.borrow().len(), 1, "un arrastre, un movimiento");
    }

    /// A strip is reordered along itself: what the other axis says about the pointer says nothing about where
    /// the item lands, and carrying it only lets the one in hand leave the line the strip lives on.
    #[test]
    fn a_pill_does_not_leave_the_line_the_strip_is_on() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);

        route(&mut tree, &press(10.0, 20.0));
        route(&mut tree, &moved(160.0, 400.0));

        let mut down = Vec::new();
        renderer_core::for_each_with_matrix(&tree.commands(), |command, matrix| {
            if let renderer_core::DrawCommand::Rect { rect, .. } = command
                && rect.width == ITEM
            {
                down.push(matrix[1] * rect.x + matrix[3] * rect.y + matrix[5]);
            }
        });
        assert!(
            down.iter().all(|y| y.abs() < 1.0),
            "una pastilla se fue hacia abajo: {down:?}"
        );
    }

    /// And no further than the strip. A pointer dragged out of the window goes on reporting, and an item that
    /// followed it there is one nobody can see to drop.
    #[test]
    fn a_pill_is_not_carried_past_the_strip() {
        fresh_layout_runtime();
        let (widget, _items, _moves) = strip();
        let node = widget.layout_node();
        let mut tree = ui_core::ComponentList::new(widget);
        lay_out_row(node, 400.0, 40.0);
        let resting = drawn_at(&tree);

        route(&mut tree, &press(10.0, 20.0));
        route(&mut tree, &moved(4000.0, 20.0));

        let carried = drawn_at(&tree);
        let travelled = carried[0] - resting[0];
        assert!(
            travelled <= 400.0,
            "la pastilla se fue de la tira: {travelled}"
        );
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
