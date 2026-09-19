use std::cell::RefCell;
use std::rc::Rc;

use platform_core::NamedKey;
use reactive_core::{Reactive, RwSignal, signal};
use renderer_core::RectStyle;
use ui_core::{ComponentList, Container, StyledContainer, box_item};

use super::*;
use crate::harness::{lay_out, moved, named, press, release, route};
use crate::test_support::fresh_layout_runtime;

const ITEM: f32 = 100.0;
const ROW: f32 = 40.0;

type Moves = Rc<RefCell<Vec<(Slot, Slot)>>>;
type Detached = Rc<RefCell<Vec<(Slot, (f32, f32))>>>;

struct Board {
    tree: ComponentList,
    items: RwSignal<Vec<Vec<char>>>,
    moves: Moves,
    detached: Detached,
    group: ReorderGroup,
}

/// Two rows stacked in a 400-wide column: `a b c` over `x y`, each pill 100x40.
fn board(detachable: bool, threshold: f32) -> Board {
    fresh_layout_runtime();
    ui_core::focus::clear();
    let items = signal(vec![vec!['a', 'b', 'c'], vec!['x', 'y']]);
    let moves: Moves = Rc::new(RefCell::new(Vec::new()));
    let detached: Detached = Rc::new(RefCell::new(Vec::new()));

    let recorded = moves.clone();
    let mut group = ReorderGroup::new()
        .drag_threshold(threshold)
        .on_move(move |from, to| {
            recorded.borrow_mut().push((from, to));
            let mut now = items.peek();
            apply_zone_move(&mut now, from, to);
            items.set(now);
        });
    if detachable {
        let dropped = detached.clone();
        group = group.on_detach(move |from, at| {
            dropped.borrow_mut().push((from, at));
            let mut now = items.peek();
            now[from.zone].remove(from.index);
            items.set(now);
        });
    }
    let zone = |index: usize| {
        group
            .zone(
                ReorderZoneProps::props()
                    .zone(index)
                    .count(Reactive::of(move || items.get()[index].len()))
                    .item(Rc::new(|_| {
                        Ok(box_item(StyledContainer::new(
                            LayoutStyle::new().width(ITEM).height(ROW),
                            |_| RectStyle::default(),
                            vec![],
                        )?))
                    }))
                    .row(true)
                    .build(),
            )
            .unwrap()
    };
    let column = Container::new(
        LayoutStyle::new().flex_column().width(400.0),
        vec![zone(0), zone(1)],
    )
    .unwrap();
    let node = column.layout_node();
    let tree = ComponentList::new(box_item(column));
    lay_out(node, 400.0, 200.0);
    Board {
        tree,
        items,
        moves,
        detached,
        group,
    }
}

impl Board {
    fn event(&mut self, event: platform_core::Event) {
        route(&mut self.tree, &event);
    }

    fn drag(&mut self, from: (f64, f64), path: &[(f64, f64)]) {
        self.event(press(from.0, from.1));
        for &(x, y) in path {
            self.event(moved(x, y));
        }
        let last = path.last().copied().unwrap_or(from);
        self.event(release(last.0, last.1));
    }

    fn line_at(&self) -> Option<(f32, f32)> {
        let mut found = None;
        renderer_core::for_each_with_matrix(&self.tree.commands(), |command, matrix| {
            if let renderer_core::DrawCommand::Rect { rect, style, .. } = command
                && rect.width == LINE
                && style.fill.is_some()
            {
                found = Some((rect.x + matrix[4], rect.y + matrix[5]));
            }
        });
        found
    }
}

#[test]
fn a_move_across_containers_is_reported_once() {
    let mut board = board(false, 0.0);
    board.drag((10.0, 20.0), &[(60.0, 20.0), (120.0, 60.0), (160.0, 60.0)]);
    assert_eq!(
        *board.moves.borrow(),
        vec![(Slot::new(0, 0), Slot::new(1, 2))]
    );
    assert_eq!(
        board.items.peek(),
        vec![vec!['b', 'c'], vec!['x', 'y', 'a']]
    );
}

#[test]
fn a_move_within_a_container_keeps_the_single_strip_rule() {
    let mut board = board(false, 0.0);
    board.drag((10.0, 20.0), &[(260.0, 20.0)]);
    assert_eq!(
        *board.moves.borrow(),
        vec![(Slot::new(0, 0), Slot::new(0, 3))]
    );
    assert_eq!(board.items.peek()[0], vec!['b', 'c', 'a']);
}

#[test]
fn the_insertion_line_marks_the_slot_in_the_container_under_the_pointer() {
    let mut board = board(false, 0.0);
    board.event(press(10.0, 20.0));
    board.event(moved(160.0, 60.0));
    assert_eq!(board.group.target(), Some(Slot::new(1, 2)));
    assert_eq!(
        board.line_at(),
        Some((2.0 * ITEM - LINE / 2.0, ROW)),
        "after the last pill of the second row"
    );

    board.event(moved(110.0, 20.0));
    assert_eq!(board.group.target(), Some(Slot::new(0, 1)));
    assert_eq!(board.line_at(), Some((ITEM - LINE / 2.0, 0.0)));
    board.event(release(110.0, 20.0));
    assert!(
        board.moves.borrow().is_empty(),
        "back over its own slot is not a move"
    );
    assert_eq!(board.line_at(), None, "the line goes with the stroke");
}

#[test]
fn dragged_out_of_every_container_an_item_detaches() {
    let mut board = board(true, 0.0);
    board.drag((110.0, 20.0), &[(120.0, 40.0), (130.0, 150.0)]);
    assert!(board.moves.borrow().is_empty());
    assert_eq!(
        *board.detached.borrow(),
        vec![(Slot::new(0, 1), (130.0, 150.0))]
    );
    assert_eq!(board.items.peek()[0], vec!['a', 'c']);
}

#[test]
fn without_a_detach_handler_a_drop_outside_changes_nothing() {
    let mut board = board(false, 0.0);
    board.event(press(110.0, 20.0));
    board.event(moved(130.0, 150.0));
    assert_eq!(board.group.target(), None);
    assert_eq!(board.line_at(), None);
    board.event(release(130.0, 150.0));
    assert!(board.moves.borrow().is_empty());
    assert_eq!(board.items.peek()[0], vec!['a', 'b', 'c']);
}

/// Items arriving mid-stroke move the live rects, not the ones the stroke is measured against: with an item pushed onto the front of the second row, `260` is past three live centres but only two frozen ones.
#[test]
fn slots_are_measured_against_the_rects_frozen_at_the_press() {
    let mut board = board(false, 0.0);
    board.event(press(10.0, 20.0));
    board.event(moved(40.0, 20.0));
    let mut grown = board.items.peek();
    grown[1].insert(0, 'z');
    board.items.set(grown);
    ui_core::relayout_if_dirty();
    board.event(moved(260.0, 60.0));
    board.event(release(260.0, 60.0));
    assert_eq!(
        *board.moves.borrow(),
        vec![(Slot::new(0, 0), Slot::new(1, 2))]
    );
}

#[test]
fn escape_mid_drag_puts_the_item_back() {
    let mut board = board(true, 0.0);
    board.event(press(10.0, 20.0));
    board.event(moved(160.0, 60.0));
    board.event(named(NamedKey::Escape));
    assert_eq!(board.group.carried(), None);
    board.event(moved(170.0, 150.0));
    board.event(release(170.0, 150.0));
    assert!(board.moves.borrow().is_empty());
    assert!(board.detached.borrow().is_empty());
}

#[test]
fn the_keyboard_moves_an_item_across_containers() {
    let mut board = board(false, 4.0);
    board.event(press(110.0, 20.0));
    board.event(release(110.0, 20.0));

    board.event(named(NamedKey::Space));
    assert_eq!(board.group.carried(), Some(Slot::new(0, 1)));
    board.event(named(NamedKey::ArrowRight));
    assert_eq!(
        board.group.target(),
        Some(Slot::new(0, 3)),
        "past `c`, skipping the slot it already fills"
    );
    board.event(named(NamedKey::ArrowRight));
    assert_eq!(
        board.group.target(),
        Some(Slot::new(1, 0)),
        "into the next container"
    );
    board.event(named(NamedKey::ArrowDown));
    assert_eq!(board.group.target(), Some(Slot::new(1, 1)));
    board.event(named(NamedKey::Enter));

    assert_eq!(
        *board.moves.borrow(),
        vec![(Slot::new(0, 1), Slot::new(1, 1))]
    );
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'c'], vec!['x', 'b', 'y']]
    );
    assert_eq!(board.group.carried(), None);
}

#[test]
fn escape_puts_a_keyboard_pickup_back() {
    let mut board = board(false, 4.0);
    board.event(press(110.0, 20.0));
    board.event(release(110.0, 20.0));
    board.event(named(NamedKey::Enter));
    board.event(named(NamedKey::ArrowLeft));
    assert_eq!(board.group.target(), Some(Slot::new(0, 0)));
    board.event(named(NamedKey::Escape));
    assert_eq!(board.group.carried(), None);
    assert!(board.moves.borrow().is_empty());
}

#[test]
fn apply_zone_move_moves_between_and_within_containers() {
    let mut zones = vec![vec![1, 2, 3], vec![4]];
    assert!(apply_zone_move(
        &mut zones,
        Slot::new(0, 0),
        Slot::new(1, 1)
    ));
    assert_eq!(zones, vec![vec![2, 3], vec![4, 1]]);
    assert!(apply_zone_move(
        &mut zones,
        Slot::new(0, 0),
        Slot::new(0, 2)
    ));
    assert_eq!(zones, vec![vec![3, 2], vec![4, 1]]);
    assert!(!apply_zone_move(
        &mut zones,
        Slot::new(1, 0),
        Slot::new(1, 1)
    ));
}
