use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use telar::testing::{advance_time, hold, key_with, lay_out, moved, named, press, release, route};
use telar::{
    ComponentList, Container, Key, LayoutScrollArea, ModifiersState, NamedKey, Reactive, RectStyle,
    RwSignal, ScrollViewport, StyledContainer, box_item, signal,
};

use super::*;
use crate::test_support::fresh_layout_runtime;

const ITEM: f32 = 100.0;
const ROW: f32 = 40.0;

type Drops = Rc<RefCell<Vec<(Slot, DropPlace<Slot>)>>>;
type Detached = Rc<RefCell<Vec<(Slot, (f32, f32))>>>;

struct Board {
    tree: ComponentList,
    items: RwSignal<Vec<Vec<char>>>,
    drops: Drops,
    detached: Detached,
    group: ReorderGroup<Slot>,
    session: DragSession<Slot>,
}

fn pill() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().width(ITEM).height(ROW),
        |_| RectStyle::default(),
        vec![],
    )?))
}

/// The storage-side answer to a drop: an item put between two others moves there, and one dropped onto another is left where it was.
fn recording(
    items: RwSignal<Vec<Vec<char>>>,
    drops: &Drops,
) -> impl Fn(Slot, DropPlace<Slot>) + use<> {
    let drops = drops.clone();
    move |from, place| {
        drops.borrow_mut().push((from, place));
        if let DropPlace::Between(to) = place {
            let mut now = items.peek();
            apply_zone_move(&mut now, from, to);
            items.set(now);
        }
    }
}

/// Two rows stacked in a 400-wide column, `a b c` over `x y`, each pill 100x40, and what `beneath` builds under them sharing their session.
fn board_with(
    detachable: bool,
    threshold: f32,
    shape: impl FnOnce(ReorderGroup<Slot>) -> ReorderGroup<Slot>,
    beneath: impl FnOnce(&DragSession<Slot>) -> Vec<Box<dyn LayoutItem>>,
) -> Board {
    fresh_layout_runtime();
    telar::focus::clear();
    let items = signal(vec![vec!['a', 'b', 'c'], vec!['x', 'y']]);
    let drops: Drops = Rc::default();
    let detached: Detached = Rc::default();
    let session = DragSession::new();

    let mut group = ReorderGroup::new(&session, |slot| slot)
        .drag_threshold(threshold)
        .on_drop(recording(items, &drops));
    if detachable {
        let dropped = detached.clone();
        group = group.on_detach(move |from, at| {
            dropped.borrow_mut().push((from, at));
            let mut now = items.peek();
            now[from.zone].remove(from.index);
            items.set(now);
        });
    }
    let group = shape(group);
    let zone = |index: usize| {
        group
            .zone(
                ReorderZoneProps::props()
                    .zone(index)
                    .count(Reactive::of(move || items.get()[index].len()))
                    .item(Rc::new(|_| pill()))
                    .row(true)
                    .build(),
            )
            .unwrap()
    };
    let mut rows = vec![zone(0), zone(1)];
    rows.extend(beneath(&session));
    let column = Container::new(LayoutStyle::new().flex_column().width(400.0), rows).unwrap();
    let node = column.layout_node();
    let tree = ComponentList::new(box_item(column));
    lay_out(node, 400.0, 200.0);
    Board {
        tree,
        items,
        drops,
        detached,
        group,
        session,
    }
}

fn board(detachable: bool, threshold: f32) -> Board {
    board_with(detachable, threshold, |group| group, |_| Vec::new())
}

fn alt() -> ModifiersState {
    hold(false, true)
}

impl Board {
    fn event(&mut self, event: telar::Event) {
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

    fn click(&mut self, at: (f64, f64)) {
        self.event(press(at.0, at.1));
        self.event(release(at.0, at.1));
    }

    fn step(&mut self, key: NamedKey) {
        self.event(key_with(Key::Named(key), alt()));
        hold(false, false);
    }

    /// Whether the insertion rule is drawn: a filled bar as thin as a focus ring.
    fn draws_a_line(&self) -> bool {
        let mut found = false;
        telar::for_each_with_matrix(&self.tree.commands(), |command, _| {
            if let telar::DrawCommand::Rect { rect, style, .. } = command
                && rect.width == crate::shared::focus_ring_width()
                && style.fill.is_some()
            {
                found = true;
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
        *board.drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(1, 2)))]
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
        *board.drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(0, 3)))]
    );
    assert_eq!(board.items.peek()[0], vec!['b', 'c', 'a']);
}

#[test]
fn the_insertion_line_follows_the_gap_under_the_pointer() {
    let mut board = board(false, 0.0);
    assert!(!board.draws_a_line(), "no line with nothing in hand");
    board.event(press(10.0, 20.0));
    board.event(moved(160.0, 60.0));
    assert_eq!(
        board.group.target(),
        Some(DropPlace::Between(Slot::new(1, 2)))
    );
    assert!(board.draws_a_line());

    board.event(moved(110.0, 20.0));
    assert_eq!(
        board.group.target(),
        Some(DropPlace::Between(Slot::new(0, 1)))
    );
    board.event(release(110.0, 20.0));
    assert!(
        board.drops.borrow().is_empty(),
        "back over its own slot is not a move"
    );
    assert!(!board.draws_a_line(), "the line goes with the stroke");
}

#[test]
fn dragged_out_of_every_container_an_item_detaches() {
    let mut board = board(true, 0.0);
    board.drag((110.0, 20.0), &[(120.0, 40.0), (130.0, 150.0)]);
    assert!(board.drops.borrow().is_empty());
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
    assert!(!board.draws_a_line());
    board.event(release(130.0, 150.0));
    assert!(board.drops.borrow().is_empty());
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
    telar::relayout_if_dirty();
    board.event(moved(260.0, 60.0));
    board.event(release(260.0, 60.0));
    assert_eq!(
        *board.drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(1, 2)))]
    );
}

#[test]
fn escape_mid_drag_puts_the_item_back() {
    let mut board = board(true, 0.0);
    board.event(press(10.0, 20.0));
    board.event(moved(160.0, 60.0));
    board.event(named(NamedKey::Escape));
    assert_eq!(board.group.carried(), None);
    assert_eq!(board.session.carried(), None);
    assert!(!board.draws_a_line());
    board.event(moved(170.0, 150.0));
    board.event(release(170.0, 150.0));
    assert!(board.drops.borrow().is_empty());
    assert!(board.detached.borrow().is_empty());
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'b', 'c'], vec!['x', 'y']]
    );
}

/// The second row takes things dropped onto its items; the gaps take them everywhere.
fn into_the_second_row(group: ReorderGroup<Slot>) -> ReorderGroup<Slot> {
    group.accepts(|_, place| match place {
        DropPlace::Inside(slot) => slot.zone == 1,
        DropPlace::Between(_) => true,
    })
}

#[test]
fn an_item_dropped_onto_one_that_accepts_it_is_reported_inside() {
    let mut board = board_with(false, 0.0, into_the_second_row, |_| Vec::new());
    board.event(press(10.0, 20.0));
    board.event(moved(50.0, 60.0));
    assert_eq!(
        board.group.target(),
        Some(DropPlace::Inside(Slot::new(1, 0)))
    );
    assert!(!board.draws_a_line(), "onto an item is shown on the item");
    board.event(moved(15.0, 60.0));
    assert_eq!(
        board.group.target(),
        Some(DropPlace::Between(Slot::new(1, 0))),
        "the outer quarter of an item is still the gap beside it"
    );
    board.event(moved(50.0, 60.0));
    board.event(release(50.0, 60.0));
    assert_eq!(
        *board.drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Inside(Slot::new(1, 0)))]
    );
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'b', 'c'], vec!['x', 'y']],
        "a drop onto an item is the caller's to carry out"
    );
}

#[test]
fn over_an_item_that_refuses_it_the_drop_lands_in_the_gap_beside_it() {
    let mut board = board_with(false, 0.0, into_the_second_row, |_| Vec::new());
    board.drag((10.0, 20.0), &[(60.0, 20.0), (240.0, 20.0)]);
    assert_eq!(
        *board.drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(0, 2)))],
        "the first row takes nothing onto its items, so the middle of `c` is the gap before it"
    );
}

#[test]
fn an_item_is_never_dropped_onto_itself() {
    let mut board = board_with(
        false,
        0.0,
        |group| group.accepts(|_, _| true),
        |_| Vec::new(),
    );
    board.event(press(10.0, 20.0));
    board.event(moved(50.0, 20.0));
    assert_ne!(
        board.group.target(),
        Some(DropPlace::Inside(Slot::new(0, 0)))
    );
    board.event(release(50.0, 20.0));
    assert!(board.drops.borrow().is_empty());
}

type Taken = Rc<RefCell<Vec<(Slot, DropPlace<usize>)>>>;
type Held = Rc<RefCell<Vec<usize>>>;

/// A 100x40 box under the rows, built apart from the group, that takes any payload onto itself, recording what it took and which of its items the pointer was held over.
fn bin(
    taken: &Taken,
    held: &Held,
) -> impl FnOnce(&DragSession<Slot>) -> Vec<Box<dyn LayoutItem>> + use<> {
    let (taken, held) = (taken.clone(), held.clone());
    move |session| {
        let bin = StyledContainer::new(
            LayoutStyle::new().width(ITEM).height(ROW),
            |_| RectStyle::default(),
            vec![],
        )
        .unwrap();
        let node = bin.layout_node();
        let rect = track_layout(node).unwrap();
        session.area(
            DropArea::new(
                node,
                Axis::Vertical,
                |_, place| place == DropPlace::Inside(0),
                move |payload, place| taken.borrow_mut().push((payload, place)),
            )
            .layout(move || {
                let at = rect.peek();
                AreaLayout {
                    bounds: at,
                    items: vec![at],
                }
            })
            .on_hold(move |_, index| held.borrow_mut().push(index)),
        );
        vec![box_item(bin)]
    }
}

#[test]
fn an_item_lands_on_an_area_another_widget_registered_in_the_same_session() {
    let (taken, held) = (Taken::default(), Held::default());
    let mut board = board_with(false, 0.0, |group| group, bin(&taken, &held));
    board.drag((110.0, 20.0), &[(120.0, 40.0), (50.0, 100.0)]);
    assert_eq!(
        *taken.borrow(),
        vec![(Slot::new(0, 1), DropPlace::Inside(0))]
    );
    assert!(
        board.drops.borrow().is_empty(),
        "the group hears only of what lands in it"
    );
}

#[test]
fn an_area_hears_once_that_the_pointer_was_held_over_an_item() {
    let (taken, held) = (Taken::default(), Held::default());
    let mut board = board_with(false, 0.0, |group| group, bin(&taken, &held));
    board.event(press(110.0, 20.0));
    board.event(moved(50.0, 100.0));
    advance_time(Duration::from_millis(300));
    assert!(held.borrow().is_empty(), "not before the delay");
    board.event(moved(55.0, 100.0));
    advance_time(Duration::from_millis(500));
    assert_eq!(
        *held.borrow(),
        vec![0],
        "moving over the same item keeps the wait"
    );
    advance_time(Duration::from_secs(2));
    assert_eq!(*held.borrow(), vec![0], "once per stay");

    board.event(moved(50.0, 20.0));
    board.event(moved(50.0, 100.0));
    board.event(named(NamedKey::Escape));
    advance_time(Duration::from_secs(2));
    assert_eq!(*held.borrow(), vec![0], "a cancelled stroke holds nothing");
}

#[test]
fn alt_and_the_arrows_step_the_focused_item_through_the_gaps() {
    let mut board = board(false, 4.0);
    board.click((110.0, 20.0));

    board.step(NamedKey::ArrowDown);
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'c', 'b'], vec!['x', 'y']],
        "past `c`, skipping the gap it already fills"
    );
    board.step(NamedKey::ArrowDown);
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'c'], vec!['b', 'x', 'y']],
        "into the next container, focus having gone with it"
    );
    board.step(NamedKey::ArrowUp);
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'c', 'b'], vec!['x', 'y']]
    );
    assert!(
        board
            .drops
            .borrow()
            .iter()
            .all(|(_, place)| matches!(place, DropPlace::Between(_))),
        "a step is reported as a drop between two items"
    );
}

#[test]
fn enter_space_and_the_bare_arrows_are_left_to_the_caller() {
    let mut board = board(false, 4.0);
    board.click((110.0, 20.0));
    for key in [
        NamedKey::Enter,
        NamedKey::Space,
        NamedKey::ArrowDown,
        NamedKey::ArrowRight,
    ] {
        board.event(named(key));
    }
    assert!(board.drops.borrow().is_empty());
    assert_eq!(board.group.carried(), None, "nothing was picked up");
    assert_eq!(
        board.items.peek(),
        vec![vec!['a', 'b', 'c'], vec!['x', 'y']]
    );
}

#[test]
fn the_keyboard_stops_at_the_first_gap_instead_of_detaching() {
    let mut board = board(true, 4.0);
    board.click((10.0, 20.0));
    board.step(NamedKey::ArrowUp);
    assert!(board.drops.borrow().is_empty());
    assert!(board.detached.borrow().is_empty());
    assert_eq!(board.items.peek()[0], vec!['a', 'b', 'c']);
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

const LONG: usize = 12;

type LongList = (
    ComponentList,
    RwSignal<Vec<Vec<char>>>,
    Drops,
    ScrollViewport,
);

/// A column of twelve 40px pills in a scroll area 200px tall, at the top of a 400x300 surface.
fn long_list() -> LongList {
    fresh_layout_runtime();
    telar::focus::clear();
    let items = signal(vec![('a'..='l').collect::<Vec<char>>()]);
    let drops: Drops = Rc::default();
    let session = DragSession::new();
    let group = ReorderGroup::new(&session, |slot| slot)
        .drag_threshold(0.0)
        .on_drop(recording(items, &drops));
    let mut viewport = None;
    let scroll =
        LayoutScrollArea::new_with(LayoutStyle::new().width(ITEM).height(200.0), |handed| {
            viewport = Some(handed);
            group.zone(
                ReorderZoneProps::props()
                    .count(Reactive::of(move || items.get()[0].len()))
                    .item(Rc::new(|_| pill()))
                    .build(),
            )
        })
        .unwrap();
    let column = Container::new(
        LayoutStyle::new().flex_column().width(400.0),
        vec![box_item(scroll)],
    )
    .unwrap();
    let node = column.layout_node();
    let tree = ComponentList::new(box_item(column));
    lay_out(node, 400.0, 300.0);
    (tree, items, drops, viewport.expect("the content was built"))
}

#[test]
fn held_near_the_bottom_edge_the_list_scrolls_and_the_drop_lands_past_what_was_in_view() {
    let (mut tree, items, drops, viewport) = long_list();
    route(&mut tree, &press(50.0, 20.0));
    route(&mut tree, &moved(50.0, 100.0));
    route(&mut tree, &moved(50.0, 195.0));
    for _ in 0..20 {
        advance_time(Duration::from_millis(50));
    }
    assert!(
        viewport.peek_offset().1 > 0.0,
        "the list scrolled under the pointer"
    );
    route(&mut tree, &release(50.0, 195.0));
    assert_eq!(
        *drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(0, LONG)))]
    );
    assert_eq!(items.peek()[0].last(), Some(&'a'));
}

#[test]
fn away_from_the_edges_the_list_stays_where_it_is() {
    let (mut tree, _items, drops, viewport) = long_list();
    route(&mut tree, &press(50.0, 20.0));
    route(&mut tree, &moved(50.0, 100.0));
    for _ in 0..20 {
        advance_time(Duration::from_millis(50));
    }
    assert_eq!(viewport.peek_offset(), (0.0, 0.0));
    route(&mut tree, &release(50.0, 100.0));
    assert_eq!(
        *drops.borrow(),
        vec![(Slot::new(0, 0), DropPlace::Between(Slot::new(0, 3)))]
    );
}

#[test]
fn escape_after_scrolling_still_puts_the_item_back() {
    let (mut tree, items, drops, viewport) = long_list();
    route(&mut tree, &press(50.0, 20.0));
    route(&mut tree, &moved(50.0, 195.0));
    advance_time(Duration::from_millis(50));
    advance_time(Duration::from_millis(50));
    route(&mut tree, &named(NamedKey::Escape));
    let stopped = viewport.peek_offset();
    advance_time(Duration::from_millis(500));
    assert_eq!(
        viewport.peek_offset(),
        stopped,
        "the scroll stops with the stroke"
    );
    route(&mut tree, &release(50.0, 195.0));
    assert!(drops.borrow().is_empty());
    assert_eq!(items.peek()[0].first(), Some(&'a'));
}

fn drawn_texts(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_label_of_the_item_in_hand_is_drawn_only_while_it_is_carried() {
    let mut board = board_with(
        false,
        0.0,
        |group| group.label(|slot| format!("item {}:{}", slot.zone, slot.index)),
        |_| Vec::new(),
    );
    assert!(drawn_texts(&board.tree).is_empty());
    board.event(press(110.0, 20.0));
    board.event(moved(160.0, 60.0));
    telar::relayout_if_dirty();
    assert_eq!(drawn_texts(&board.tree), ["item 0:1"]);
    assert_eq!(board.session.label().as_deref(), Some("item 0:1"));
    board.event(release(160.0, 60.0));
    telar::relayout_if_dirty();
    assert!(drawn_texts(&board.tree).is_empty());
}
