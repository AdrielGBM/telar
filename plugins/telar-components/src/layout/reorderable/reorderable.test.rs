use std::cell::RefCell;
use std::rc::Rc;

use telar::preview::{Play, PreviewCtx, PreviewEntry, by_role};
use telar::testing::{lay_out_row, moved, press, release, route};
use telar::{
    Component, Key, LayoutItem, LayoutStyle, NamedKey, RectStyle, RwSignal, StyledContainer, Text,
    box_item, focus::Role, signal,
};

use super::*;
use crate::test_support::fresh_layout_runtime;

const ITEM: f32 = 100.0;
const NAMES: [&str; 4] = ["Inbox", "Drafts", "Sent", "Archive"];

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
            .item(Rc::new(|_| {
                Ok(box_item(StyledContainer::new(
                    LayoutStyle::new().width(ITEM).height(40.0),
                    |_| RectStyle::default(),
                    vec![],
                )?))
            }))
            .on_move(Rc::new(move |from, to| {
                recorded.borrow_mut().push((from, to));
                let mut current = moved_items.peek();
                telar::apply_move(&mut current, from, to);
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

/// A row of four named, fixed-width items over storage of the caller's, logging each move as `on_move`.
fn named_strip(
    ctx: &PreviewCtx,
    drop_preview: DropPreview,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let items = signal(NAMES.to_vec());
    let actions = ctx.actions();
    reorderable(
        ReorderableProps::props()
            .count(Reactive::of(move || items.get().len()))
            .item(Rc::new(move |index| {
                let label = Text::declaring(
                    move || items.get()[index].to_string(),
                    LayoutStyle::new(),
                    |text| text,
                )?;
                Ok(box_item(StyledContainer::new(
                    LayoutStyle::new().width(ITEM).height(40.0),
                    |_| RectStyle::default(),
                    vec![box_item(label)],
                )?))
            }))
            .on_move(Rc::new(move |from, to| {
                actions.log("on_move", vec![from.to_string(), to.to_string()]);
                items.update(|items| {
                    telar::apply_move(items, from, to);
                });
            }))
            .row(true)
            .drop_preview(drop_preview)
            .build(),
    )
}

fn gap_strip(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    named_strip(ctx, DropPreview::Gap)
}

fn line_strip(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    named_strip(ctx, DropPreview::Line)
}

fn played(build: fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError>) -> Play {
    fresh_layout_runtime();
    telar::focus::clear();
    let entry = PreviewEntry::new(
        "test--reorderable--behaviour",
        "reorderable",
        "behaviour",
        build,
    )
    .locale("en");
    Play::mount(&entry).unwrap_or_else(|error| panic!("the strip mounts: {error}"))
}

fn item(name: &str) -> telar::preview::Query {
    by_role(Role::ListItem).named(name)
}

fn key(play: &mut Play, key: NamedKey) {
    play.press(Key::Named(key)).unwrap();
}

/// The items' names in the order a reader meets them.
fn read_order(play: &Play) -> Vec<String> {
    play.find_all(&by_role(Role::ListItem))
        .into_iter()
        .map(|node| node.name)
        .collect()
}

/// The items' names in the order they are drawn along the row, which mid-stroke is not the order they are stored in.
fn drawn_order(play: &Play) -> Vec<String> {
    let mut items = play.find_all(&by_role(Role::ListItem));
    items.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    items.into_iter().map(|node| node.name).collect()
}

fn moves(play: &Play) -> Vec<Vec<String>> {
    play.actions()
        .calls()
        .into_iter()
        .filter(|call| call.name == "on_move")
        .map(|call| call.args)
        .collect()
}

/// Whether the insertion rule is drawn: a filled bar as thin as a focus ring.
fn draws_an_insertion_line(play: &Play) -> bool {
    play.frame().commands.iter().any(|command| {
        matches!(command, telar::DrawCommand::Rect { rect, style, .. }
            if rect.width == crate::shared::focus_ring_width() && style.fill.is_some())
    })
}

#[test]
fn the_list_and_its_items_are_announced_as_such() {
    let play = played(gap_strip);
    assert_eq!(play.find_all(&by_role(Role::List)).len(), 1);
    assert_eq!(read_order(&play), NAMES);
}

#[test]
fn the_keyboard_picks_an_item_up_moves_it_and_drops_it() {
    let mut play = played(gap_strip);
    play.click(item("Inbox")).unwrap();
    play.expect_focused(item("Inbox")).unwrap();

    key(&mut play, NamedKey::Space);
    key(&mut play, NamedKey::ArrowRight);
    key(&mut play, NamedKey::Space);

    assert_eq!(
        moves(&play),
        [["0", "2"]],
        "past `Drafts`, skipping the slot it already fills"
    );
    assert_eq!(read_order(&play), ["Drafts", "Inbox", "Sent", "Archive"]);
}

#[test]
fn escape_puts_a_keyboard_pickup_back() {
    let mut play = played(gap_strip);
    play.click(item("Drafts")).unwrap();

    key(&mut play, NamedKey::Enter);
    key(&mut play, NamedKey::ArrowRight);
    key(&mut play, NamedKey::ArrowRight);
    key(&mut play, NamedKey::Escape);

    assert!(moves(&play).is_empty());
    assert_eq!(read_order(&play), NAMES);
    assert_eq!(
        drawn_order(&play),
        NAMES,
        "nothing is left drawn out of place"
    );

    key(&mut play, NamedKey::Enter);
    key(&mut play, NamedKey::ArrowLeft);
    key(&mut play, NamedKey::Enter);
    assert_eq!(moves(&play), [["1", "0"]], "the next pickup starts afresh");
}

#[test]
fn the_keyboard_stops_at_the_ends_of_the_list() {
    let mut play = played(gap_strip);
    play.click(item("Inbox")).unwrap();

    key(&mut play, NamedKey::Space);
    key(&mut play, NamedKey::ArrowLeft);
    key(&mut play, NamedKey::Space);
    assert!(
        moves(&play).is_empty(),
        "before the first slot is still the first slot"
    );

    key(&mut play, NamedKey::Space);
    for _ in 0..8 {
        key(&mut play, NamedKey::ArrowDown);
    }
    key(&mut play, NamedKey::Space);
    assert_eq!(moves(&play), [["0", "4"]]);
    assert_eq!(read_order(&play), ["Drafts", "Sent", "Archive", "Inbox"]);
}

#[test]
fn a_pointer_drag_reports_where_the_item_came_from_and_where_it_lands() {
    let mut play = played(gap_strip);
    play.drag(item("Inbox"), 2.5 * ITEM, 0.0).unwrap();
    assert_eq!(moves(&play), [["0", "3"]]);
    assert_eq!(read_order(&play), ["Drafts", "Sent", "Inbox", "Archive"]);
}

#[test]
fn escape_mid_drag_puts_the_item_back() {
    fresh_layout_runtime();
    let (widget, items, moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(260.0, 20.0));
    route(&mut tree, &telar::testing::named(NamedKey::Escape));
    route(&mut tree, &release(260.0, 20.0));

    assert!(moves.borrow().is_empty(), "{:?}", moves.borrow());
    assert_eq!(items.peek(), vec!['a', 'b', 'c', 'd']);
}

#[test]
fn with_a_gap_the_list_shows_the_order_the_drop_would_leave() {
    let mut play = played(gap_strip);
    play.click(item("Inbox")).unwrap();
    key(&mut play, NamedKey::Space);
    key(&mut play, NamedKey::ArrowRight);
    key(&mut play, NamedKey::ArrowRight);

    assert_eq!(drawn_order(&play), ["Drafts", "Sent", "Inbox", "Archive"]);
    assert!(moves(&play).is_empty(), "nothing is moved until the drop");
    assert!(!draws_an_insertion_line(&play));
}

#[test]
fn with_a_line_nothing_moves_until_the_drop() {
    let mut play = played(line_strip);
    play.click(item("Inbox")).unwrap();
    assert!(
        !draws_an_insertion_line(&play),
        "no line with nothing in hand"
    );
    key(&mut play, NamedKey::Space);
    key(&mut play, NamedKey::ArrowRight);
    key(&mut play, NamedKey::ArrowRight);

    assert_eq!(drawn_order(&play), NAMES);
    assert!(draws_an_insertion_line(&play));

    key(&mut play, NamedKey::Space);
    assert_eq!(moves(&play), [["0", "3"]]);
    assert!(
        !draws_an_insertion_line(&play),
        "the line goes with the stroke"
    );
}

/// Dragging the first pill past the centre of the third must land it there, and must report the slot in the frame of reference `apply_move` reads — which is what makes the two halves agree.
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
fn drawn_at(tree: &telar::ComponentList) -> Vec<f32> {
    let mut found = Vec::new();
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, .. } = command
            && rect.width == ITEM
        {
            found.push(matrix[0] * rect.x + matrix[2] * rect.y + matrix[4]);
        }
    });
    found
}

/// **The one being dragged goes with the pointer.** The gap says where it will land; nothing said what was going into it, so the strip opened a hole beside a pill that had not moved — which reads as two of the same pill rather than as one being carried.
#[test]
fn the_pill_being_dragged_travels_with_the_pointer() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);
    let resting = drawn_at(&tree);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(60.0, 20.0));
    route(&mut tree, &moved(160.0, 20.0));

    let carried = drawn_at(&tree);
    assert_eq!(
        carried[0] - resting[0],
        150.0,
        "the pill did not follow the pointer: {resting:?} → {carried:?}"
    );

    route(&mut tree, &release(160.0, 20.0));
    let landed = drawn_at(&tree);
    assert!(
        landed.iter().all(|x| resting.contains(x)),
        "a pill was left outside its slot: {landed:?}"
    );
}

/// **One hole, not two.** The pill travels with the pointer, so the slot it came out of has to close behind it: leaving it open showed a hole where it was *and* the gap where it is going, with nothing in either — which reads as the strip having lost one.
#[test]
fn the_slot_the_pill_came_out_of_closes_behind_it() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);
    let resting = drawn_at(&tree);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(260.0, 20.0));
    telar::relayout_if_dirty();

    let carried = drawn_at(&tree);
    assert_eq!(
        carried[2] - resting[2],
        -ITEM,
        "the slot it came out of did not close: {resting:?} → {carried:?}"
    );
}

/// **A strip mid-drag is the width it was.** The slot the carried item came out of closes by exactly what the gap it is heading for opens, so the strip never grows a second hole — which is what the last item dragged past the end did: it was both the one being carried and the one the trailing gap belongs to, and the two rules overwrote each other instead of adding up.
#[test]
fn the_strip_is_the_width_it_was_wherever_the_drop_is_heading() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let strip_rect = telar::track_layout(node).unwrap();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);
    let resting = strip_rect.get().width;

    route(&mut tree, &press(350.0, 20.0));
    for at in [360.0, 390.0, 4000.0, 360.0] {
        route(&mut tree, &moved(at, 20.0));
        telar::relayout_if_dirty();
        assert_eq!(
            strip_rect.get().width,
            resting,
            "the strip grew an extra hole with the pointer at {at}"
        );
    }
}

/// **Leftwards too.** The gap opens in front of the one in hand and moves it along with everything else after it, so a translate measured from where it is laid out carried the gap as well as the pointer: dragging left, the pill jumped a whole slot to the right the instant the gap appeared.
#[test]
fn a_pill_dragged_leftwards_follows_the_pointer_and_not_the_gap() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);
    let resting = drawn_at(&tree);

    route(&mut tree, &press(250.0, 20.0));
    route(&mut tree, &moved(40.0, 20.0));
    telar::relayout_if_dirty();

    let carried = drawn_at(&tree);
    assert_eq!(
        carried[4] - resting[4],
        -210.0,
        "the pill did not follow the pointer leftwards: {resting:?} → {carried:?}"
    );
}

/// **And it stands still where the pointer does.** The bounds were read off the live rects, which the gap moves — so at the edge, where the clamp is what decides the point, the strip fed its own gap back into the slot it was computing and swapped the pill with its neighbour for ever.
#[test]
fn a_pill_held_against_the_edge_stays_where_it_is_put() {
    fresh_layout_runtime();
    let (widget, _items, moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(4000.0, 20.0));
    telar::relayout_if_dirty();
    let against = drawn_at(&tree);

    for _ in 0..4 {
        route(&mut tree, &moved(4000.0, 20.0));
        telar::relayout_if_dirty();
        assert_eq!(
            drawn_at(&tree),
            against,
            "the strip moves on its own with the pointer still at the edge"
        );
    }

    route(&mut tree, &release(4000.0, 20.0));
    assert_eq!(*moves.borrow(), vec![(0, 4)], "one drag, one move");
}

/// A strip is reordered along itself: what the other axis says about the pointer says nothing about where the item lands, and carrying it only lets the one in hand leave the line the strip lives on.
#[test]
fn a_pill_does_not_leave_the_line_the_strip_is_on() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(160.0, 400.0));

    let mut down = Vec::new();
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, .. } = command
            && rect.width == ITEM
        {
            down.push(matrix[1] * rect.x + matrix[3] * rect.y + matrix[5]);
        }
    });
    assert!(
        down.iter().all(|y| y.abs() < 1.0),
        "a pill left the line: {down:?}"
    );
}

/// And no further than the strip. A pointer dragged out of the window goes on reporting, and an item that followed it there is one nobody can see to drop.
#[test]
fn a_pill_is_not_carried_past_the_strip() {
    fresh_layout_runtime();
    let (widget, _items, _moves) = strip();
    let node = widget.layout_node();
    let mut tree = telar::ComponentList::new(widget);
    lay_out_row(node, 400.0, 40.0);
    let resting = drawn_at(&tree);

    route(&mut tree, &press(10.0, 20.0));
    route(&mut tree, &moved(4000.0, 20.0));

    let carried = drawn_at(&tree);
    let travelled = carried[0] - resting[0];
    assert!(travelled <= 400.0, "the pill left the strip: {travelled}");
}

/// A press that never left the pill is a click on it, not a reorder — the case that makes a strip of buttons still usable as buttons.
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

/// Dropping an item back over its own slot is not a move. Reporting one would rewrite the caller's list (and every signal reading it) for a gesture that changed nothing.
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
