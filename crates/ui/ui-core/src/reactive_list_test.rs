use super::*;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use reactive_core::signal;

fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(10.0).height(10.0),
        vec![],
    )?))
}

#[test]
fn builds_initial_items() {
    reset_layout_runtime();
    let items = signal(vec![1, 2, 3]);
    let src = items;
    let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
    assert_eq!(list.state.borrow().children.len(), 3);
}

#[test]
fn reconcile_reuses_nodes_on_reorder_and_remove() {
    reset_layout_runtime();
    let items = signal(vec![1, 2, 3]);
    let src = items;
    let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
    let v1: Vec<NodeId> = list
        .state
        .borrow()
        .children
        .iter()
        .map(|c| c.node())
        .collect();
    assert_eq!(v1.len(), 3);

    items.set(vec![3, 1]);

    let st = list.state.borrow();
    assert_eq!(st.children.len(), 2, "item 2 should be dropped");
    let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
    assert_eq!(v2[0], v1[2], "item 3 keeps its node, moved to front");
    assert_eq!(v2[1], v1[0], "item 1 keeps its node");
}

#[test]
fn added_item_gets_laid_out_after_relayout() {
    use crate::context::{compute_layout, relayout_if_dirty, track_layout};
    use layout_core::AvailableSpace;

    reset_layout_runtime();
    let items = signal(vec![1i32, 2]);
    let src = items;
    let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
    let list_node = list.layout_node();
    compute_layout(
        list_node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    assert!(
        track_layout(list.state.borrow().children[0].node())
            .unwrap()
            .get()
            .height
            > 0.0,
        "initial items should be laid out"
    );

    items.set(vec![1, 2, 3]);
    assert_eq!(list.state.borrow().children.len(), 3, "item added");

    relayout_if_dirty();

    let n2 = list.state.borrow().children[2].node();
    assert!(
        track_layout(n2).unwrap().get().height > 0.0,
        "the newly added item must be laid out after relayout_if_dirty"
    );
}

#[test]
fn reconcile_appends_new_item() {
    reset_layout_runtime();
    let items = signal(vec![1, 2]);
    let src = items;
    let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
    let v1: Vec<NodeId> = list
        .state
        .borrow()
        .children
        .iter()
        .map(|c| c.node())
        .collect();

    items.set(vec![1, 2, 3]);

    let st = list.state.borrow();
    assert_eq!(st.children.len(), 3);
    let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
    assert_eq!(&v2[..2], &v1[..], "existing items keep their nodes");
}

#[test]
fn with_gap_spaces_items_in_layout() {
    use crate::context::compute_layout;
    use layout_core::AvailableSpace;

    reset_layout_runtime();
    let items = signal(vec![1i32, 2]);
    let src = items;
    let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 8.0).unwrap();
    let list_node = list.layout_node();
    compute_layout(
        list_node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let st = list.state.borrow();
    let y0 = track_layout(st.children[0].node()).unwrap().get().y;
    let y1 = track_layout(st.children[1].node()).unwrap().get().y;
    assert_eq!(
        y1 - y0,
        18.0,
        "each leaf is 10px tall; an 8px gap pushes the second item to 18px, not flush at 10px"
    );
}

#[test]
fn positional_reuses_nodes_on_append() {
    reset_layout_runtime();
    let items = signal(vec![1, 2]);
    let src = items;
    let list = ReactiveList::positional(move || src.get(), |_| leaf(), 0.0).unwrap();
    let v1: Vec<NodeId> = list
        .state
        .borrow()
        .children
        .iter()
        .map(|c| c.node())
        .collect();

    items.set(vec![1, 2, 3]);

    let st = list.state.borrow();
    assert_eq!(st.children.len(), 3);
    let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
    assert_eq!(
        &v2[..2],
        &v1[..],
        "the first two positions keep their nodes"
    );
}

/// The trap this exists to remove: with an owned snapshot a persisting key reuses the widget and the new value is thrown away, so a row that must follow its data has to be keyed on that data — which rebuilds it, and takes its local state with it. `keyed` lets the row keep its widget *and* see the new value.
#[test]
fn a_keyed_row_keeps_its_widget_and_still_sees_its_new_value() {
    reset_layout_runtime();
    reactive_core::reset_runtime();

    #[derive(Clone)]
    struct Row {
        id: u32,
        text: &'static str,
    }

    let rows = signal(vec![Row {
        id: 1,
        text: "before",
    }]);
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let builds = Rc::new(RefCell::new(0usize));

    let (sink, counter) = (Rc::clone(&seen), Rc::clone(&builds));
    let source = rows;
    let list = ReactiveList::keyed(
        move || source.get(),
        |row: &Row| row.id,
        move |held: reactive_core::ReadSignal<Row>| {
            *counter.borrow_mut() += 1;
            let sink = Rc::clone(&sink);
            effect(move || sink.borrow_mut().push(held.get().text));
            Ok(Box::new(crate::Container::column(vec![])?) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();

    let first = list.state.borrow().children[0].node();
    rows.set(vec![Row {
        id: 1,
        text: "after",
    }]);

    assert_eq!(*builds.borrow(), 1, "the row was built once, not rebuilt");
    assert_eq!(
        list.state.borrow().children[0].node(),
        first,
        "and kept the very node it had"
    );
    assert_eq!(
        *seen.borrow(),
        vec!["before", "after"],
        "while still seeing what it now says"
    );
}

/// A child handler is allowed to change the list it is in — a row that deletes itself, a strip that commits a drag-to-reorder. Both write a signal the source reads, which reconciles synchronously from inside this list's own `on_event`; holding the state borrow (or reading a node back through a widget that is mid-dispatch) made every one of those a panic rather than a feature.
#[test]
fn a_child_may_remove_itself_from_inside_its_own_handler() {
    use crate::context::compute_layout;
    use crate::styled_container::StyledContainer;
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};

    reset_layout_runtime();
    let items = signal(vec![1i32, 2, 3]);
    let src = items;
    let pressed = items;
    let mut list = ReactiveList::new(
        move || src.get(),
        |n: &i32| *n,
        move |n: i32| {
            let items = pressed;
            Ok(Box::new(
                StyledContainer::new(
                    LayoutStyle::new().width(50.0).height(20.0),
                    |_| Default::default(),
                    vec![],
                )?
                .on_press(move || {
                    items.set(items.peek().into_iter().filter(|x| *x != n).collect())
                }),
            ) as Box<dyn LayoutItem>)
        },
        0.0,
    )
    .unwrap();
    compute_layout(
        list.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let at = |y: f64| (25.0, y);
    let (x, y) = at(30.0);
    list.on_event(&Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    list.on_event(&Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });

    assert_eq!(items.peek(), vec![1, 3]);
    assert_eq!(list.state.borrow().children.len(), 2);
}

/// A row that leaves takes what its build made, including what a surviving handle would otherwise pin.
///
/// The escaping clone is not contrived: `hot_state` does exactly this on purpose, holding a signal past the component that made it so the value survives a reload. Under the handle refcount that is indistinguishable from a leak — the storage lives as long as the clone, and rebuilding a row ten times leaves ten of them. The owner is what tells the two apart.
#[test]
fn rebuilding_a_row_does_not_accumulate_what_its_build_created() {
    reset_layout_runtime();
    let escaped: Rc<RefCell<Vec<RwSignal<usize>>>> = Rc::new(RefCell::new(Vec::new()));
    let items = signal(vec![0usize]);
    let source = items;
    let kept = Rc::clone(&escaped);
    let list = ReactiveList::new(
        move || source.get(),
        |n: &usize| *n,
        move |n: usize| {
            let per_row = signal(n);
            kept.borrow_mut().push(per_row);
            leaf()
        },
        0.0,
    )
    .unwrap();

    let one_row = reactive_core::live_signal_count();
    for generation in 1..=10usize {
        items.set(vec![generation]);
    }

    assert_eq!(escaped.borrow().len(), 11, "every generation built a row");
    assert_eq!(
        reactive_core::live_signal_count(),
        one_row,
        "eleven rows built, one row's worth of state alive"
    );
    assert_eq!(list.state.borrow().children.len(), 1);
}

fn nested_row() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_row(),
        vec![leaf()?, leaf()?],
    )?))
}

/// A row leaving frees every node it built, not only its own: a shell churning notification rows for weeks cannot leak one per row.
#[test]
fn a_disposed_row_frees_every_node_beneath_it() {
    reset_layout_runtime();
    let items = signal(Vec::<u32>::new());
    let src = items;
    let _list = ReactiveList::new(move || src.get(), |n: &u32| *n, |_| nested_row(), 0.0).unwrap();
    let baseline = crate::context::live_node_count();

    for round in 0..10u32 {
        items.set((round * 5..round * 5 + 5).collect());
        assert_eq!(crate::context::live_node_count(), baseline + 5 * 3);
    }
    items.set(Vec::new());
    assert_eq!(crate::context::live_node_count(), baseline);
}

const EXIT_MS: u64 = 200;

fn fade() -> crate::presence::Transition {
    crate::presence::Transition::fade(motion_core::tween(
        std::time::Duration::from_millis(EXIT_MS),
        motion_core::Easing::Linear,
    ))
}

fn play(start: web_time::Instant, ms: u64) -> web_time::Instant {
    motion_core::tick(start);
    let end = start + std::time::Duration::from_millis(ms);
    motion_core::tick(end);
    end
}

fn row_nodes(list: &ReactiveList) -> Vec<NodeId> {
    list.state
        .borrow()
        .children
        .iter()
        .map(Child::node)
        .collect()
}

#[test]
fn a_list_row_removed_and_added_back_mid_exit_keeps_its_identity() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2, 3]);
    let builds = std::rc::Rc::new(std::cell::Cell::new(0));
    let list = {
        let builds = std::rc::Rc::clone(&builds);
        ReactiveList::new(
            move || items.get(),
            |n: &u32| *n,
            move |_| {
                builds.set(builds.get() + 1);
                leaf()
            },
            0.0,
        )
        .unwrap()
        .with_transition(fade())
    };
    let baseline = crate::context::live_node_count();
    let before = row_nodes(&list);

    items.set(vec![1, 3]);
    assert_eq!(row_nodes(&list), before, "the row leaves from where it was");
    assert!(list.state.borrow().children[1].is_leaving());
    assert_eq!(crate::presence::exits_in_flight().get(), 1);

    let midway = play(web_time::Instant::now(), EXIT_MS / 2);
    items.set(vec![1, 2, 3]);
    assert_eq!(row_nodes(&list), before, "the same node, in the same place");
    assert_eq!(builds.get(), 3, "never rebuilt");
    assert!(!list.state.borrow().children[1].is_leaving());
    assert_eq!(crate::presence::exits_in_flight().get(), 0);

    let back = play(midway, EXIT_MS * 2);
    items.set(vec![1, 3]);
    play(back, EXIT_MS + 50);
    assert_eq!(
        row_nodes(&list),
        vec![before[0], before[2]],
        "gone after its exit"
    );
    assert_eq!(
        crate::context::live_node_count(),
        baseline - 1,
        "its node is freed"
    );
    assert_eq!(crate::presence::exits_in_flight().get(), 0);
}

/// A pass that brings a leaving row back and then fails on a later row commits nothing, so the row is still leaving, still counted, and still goes when its exit ends.
#[test]
fn a_failed_pass_leaves_a_returning_row_leaving() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2, 3]);
    let list = ReactiveList::new(
        move || items.get(),
        |n: &u32| *n,
        |n| match n {
            100.. => panic!("row {n} failed"),
            _ => leaf(),
        },
        0.0,
    )
    .unwrap()
    .with_transition(fade());
    let before = row_nodes(&list);
    items.set(vec![1, 3]);
    assert!(list.state.borrow().children[1].is_leaving());

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        items.set(vec![1, 2, 3, 100]);
    }));
    assert!(outcome.is_err());

    assert_eq!(row_nodes(&list), before);
    assert!(
        list.state.borrow().children[1].is_leaving(),
        "the failed pass did not turn it back"
    );
    assert_eq!(crate::presence::exits_in_flight().get(), 1);
    play(web_time::Instant::now(), EXIT_MS + 50);
    assert_eq!(row_nodes(&list), vec![before[0], before[2]], "and it went");
}

#[test]
fn a_list_row_added_after_the_transition_plays_its_entrance() {
    reset_layout_runtime();
    let items = signal(vec![1u32]);
    let list = ReactiveList::new(move || items.get(), |n: &u32| *n, |_| leaf(), 0.0)
        .unwrap()
        .with_transition(fade());
    assert!(
        list.state.borrow().children[0].mount().is_none(),
        "a row built before the transition appears settled"
    );

    items.set(vec![1, 2]);
    let arriving = list.state.borrow().children[1].mount().expect("presented");
    assert_eq!(arriving.frame_now().1, 0.0, "starts transparent");
    play(web_time::Instant::now(), EXIT_MS + 50);
    assert_eq!(arriving.frame_now().1, 1.0, "and arrives");
}

/// A departed key's value handle goes with its row: a keyed list churning distinct keys for weeks holds only what is on screen.
#[test]
fn a_keyed_list_frees_the_value_of_every_key_that_leaves() {
    reset_layout_runtime();
    let items = signal(Vec::<u32>::new());
    let list = ReactiveList::keyed(
        move || items.get(),
        |n: &u32| *n,
        |_: reactive_core::ReadSignal<u32>| leaf(),
    )
    .unwrap();
    let baseline = reactive_core::live_signal_count();

    for round in 0..10u32 {
        items.set((round * 5..round * 5 + 5).collect());
    }
    items.set(Vec::new());

    assert_eq!(reactive_core::live_signal_count(), baseline);
    assert!(list.state.borrow().children.is_empty());
}

/// A key that returns mid-exit keeps its handle, and the handle carries the value it came back with.
#[test]
fn a_keyed_row_that_returns_mid_exit_keeps_its_value_handle() {
    reset_layout_runtime();
    let items = signal(vec![(1u32, "first")]);
    type Row = (u32, &'static str);
    let seen: Rc<Cell<Option<reactive_core::ReadSignal<Row>>>> = Rc::default();
    let list = {
        let seen = Rc::clone(&seen);
        ReactiveList::keyed(
            move || items.get(),
            |row: &Row| row.0,
            move |held| {
                seen.set(Some(held));
                leaf()
            },
        )
        .unwrap()
        .with_transition(fade())
    };
    let held = seen.get().expect("built");

    items.set(Vec::new());
    let midway = play(web_time::Instant::now(), EXIT_MS / 2);
    items.set(vec![(1, "again")]);
    play(midway, EXIT_MS * 2);

    assert!(held.is_alive(), "the returning row kept its handle");
    assert_eq!(held.get().1, "again");
    assert_eq!(list.state.borrow().children.len(), 1);
}

fn laid_out_height(node: NodeId) -> f32 {
    use crate::context::compute_layout;
    use layout_core::AvailableSpace;
    compute_layout(node, AvailableSpace::MaxContent, AvailableSpace::MaxContent).unwrap();
    track_layout(node).unwrap().get().height
}

/// Outside a batch a row build's write flushes on the spot, and the effect it wakes may change the list being built. The runner's open batch hides this in production; a harness building without one hit it.
#[test]
fn an_effect_woken_by_a_row_build_may_change_the_same_list() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2]);
    let built = signal(0u32);
    let _grow = reactive_core::effect(move || {
        if built.get() == 2 {
            items.update(|v| {
                if !v.contains(&3) {
                    v.push(3);
                }
            });
        }
    });
    let list = ReactiveList::new(
        move || items.get(),
        |n: &u32| *n,
        move |n| {
            built.set(n);
            leaf()
        },
        0.0,
    )
    .unwrap();

    assert_eq!(
        list.state.borrow().keys.len(),
        3,
        "the queued change applied"
    );
    assert_eq!(laid_out_height(list.layout_node()), 30.0);
    items.set(vec![1]);
    assert_eq!(list.state.borrow().children.len(), 1);
}

#[test]
fn a_row_build_that_panics_outside_a_boundary_leaves_the_list_whole() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2]);
    let list = ReactiveList::new(
        move || items.get(),
        |n: &u32| *n,
        |n| {
            assert_ne!(n, 99, "row build failed");
            leaf()
        },
        0.0,
    )
    .unwrap();
    let before = row_nodes(&list);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        items.set(vec![1, 2, 99]);
    }));
    assert!(outcome.is_err(), "no boundary caught the panic");

    assert_eq!(row_nodes(&list), before, "the committed rows stand");
    assert_eq!(
        laid_out_height(list.layout_node()),
        20.0,
        "and so does the layout"
    );

    items.set(vec![2, 3]);
    let after = row_nodes(&list);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[1], "the next reconcile still reuses rows");
    assert_eq!(laid_out_height(list.layout_node()), 20.0);
}

/// A row failing on every change, as a card might on every notification, must not leave an owner behind per attempt — nor the rows that built fine earlier in the failed pass.
#[test]
fn rows_that_fail_to_build_leave_nothing_behind() {
    use crate::test_support::{Failure, live, stateful_leaf};

    reset_layout_runtime();
    let items = signal(vec![1u32]);
    let list = ReactiveList::new(
        move || items.get(),
        |n: &u32| *n,
        |n| {
            stateful_leaf(match n {
                100.. => Failure::Panic,
                50.. => Failure::Err,
                _ => Failure::None,
            })
        },
        0.0,
    )
    .unwrap();
    let baseline = live();

    for attempt in 0..6 {
        let failing = if attempt % 2 == 0 { 100 } else { 50 } + attempt;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            items.set(vec![1, 2, failing]);
        }));
        assert!(outcome.is_err(), "no boundary caught attempt {attempt}");
        assert_eq!(live(), baseline, "attempt {attempt} left something behind");
    }

    items.set(vec![1, 2]);
    assert_eq!(list.state.borrow().children.len(), 2);
}

/// A key the source repeats names one row per occurrence, matched in order of appearance, and every one of them is tracked: reconciling a list with duplicates over and over leaves nothing behind, and a duplicate going away frees its row.
#[test]
fn duplicate_keys_keep_one_row_each_and_leak_nothing() {
    use crate::test_support::{Failure, live, stateful_leaf};

    reset_layout_runtime();
    let items = signal(vec![1u32, 1, 2]);
    let builds = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&builds);
    let list = ReactiveList::new(
        move || items.get(),
        |n: &u32| *n,
        move |_| {
            counted.set(counted.get() + 1);
            stateful_leaf(Failure::None)
        },
        0.0,
    )
    .unwrap();
    let rows = row_nodes(&list);
    assert_eq!(rows.len(), 3);
    let baseline = live();

    for _ in 0..3 {
        items.set(vec![1, 2, 1]);
        items.set(vec![1, 1, 2]);
    }
    assert_eq!(
        builds.get(),
        3,
        "every row, duplicates included, was reused"
    );
    assert_eq!(row_nodes(&list), rows);
    assert_eq!(live(), baseline, "and nothing accumulated");

    let (nodes, owners) = (
        crate::context::live_node_count(),
        reactive_core::live_owner_count(),
    );
    items.set(vec![1, 2]);
    assert_eq!(
        row_nodes(&list),
        vec![rows[0], rows[2]],
        "the later duplicate went"
    );
    assert!(
        crate::context::live_node_count() < nodes,
        "and its nodes were freed"
    );
    assert!(
        reactive_core::live_owner_count() < owners,
        "and its owner disposed"
    );
}

/// With duplicate keys each row still follows its own occurrence's value.
#[test]
fn a_keyed_list_with_duplicate_keys_gives_each_row_its_own_value() {
    reset_layout_runtime();
    type Row = (u32, &'static str);
    let rows = signal(vec![(1u32, "a"), (1, "b")]);
    let seen = Rc::new(RefCell::new(Vec::<reactive_core::ReadSignal<Row>>::new()));
    let sink = Rc::clone(&seen);
    let _list = ReactiveList::keyed(
        move || rows.get(),
        |row: &Row| row.0,
        move |held| {
            sink.borrow_mut().push(held);
            Ok(Box::new(crate::Container::column(vec![])?) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();

    rows.set(vec![(1, "c"), (1, "d")]);

    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].get().1, "c");
    assert_eq!(seen[1].get().1, "d");
}

mod layout_animation {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use layout_core::AvailableSpace;
    use motion_core::{Easing, Spring, set_scale, tick, tween};
    use platform_core::{PointerButton, PointerSource};
    use renderer_core::RectStyle;
    use web_time::Instant;

    use super::*;
    use crate::context::{compute_layout, live_node_count, relayout_if_dirty};
    use crate::layout_item::box_item;
    use crate::styled_container::StyledContainer;
    use crate::surface::IDENTITY;

    const SLIDE_MS: u64 = 200;

    fn slide() -> motion_core::Tween {
        tween(Duration::from_millis(SLIDE_MS), Easing::Linear)
    }

    fn fresh() {
        reset_layout_runtime();
        motion_core::reset();
        set_scale(1.0);
    }

    fn lay_out(list: &ReactiveList) {
        compute_layout(
            list.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
    }

    fn keyed_list(items: RwSignal<Vec<u32>>) -> ReactiveList {
        ReactiveList::new(move || items.get(), |n: &u32| *n, |_| leaf(), 0.0).unwrap()
    }

    fn row(list: &ReactiveList, index: usize) -> Child {
        list.state.borrow().children[index].clone()
    }

    fn laid_out_y(child: &Child) -> f32 {
        track_layout(child.node()).unwrap().peek().y
    }

    fn drawn_y(child: &Child) -> f32 {
        laid_out_y(child) + child.matrix_now()[5]
    }

    /// One frame as the runner drives it: the animations advance, then whatever they dirtied is laid out.
    fn frame(now: Instant, ms: u64) -> Instant {
        let next = now + Duration::from_millis(ms);
        tick(next);
        relayout_if_dirty();
        next
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-3
    }

    #[test]
    fn a_reorder_slides_the_rows_by_transform_without_laying_out_again() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items).animate_layout(slide());
        lay_out(&list);
        assert_eq!(
            row(&list, 0).matrix_now(),
            IDENTITY,
            "the first layout places the rows without sliding them in from nowhere"
        );

        items.set(vec![3, 1, 2]);
        relayout_if_dirty();
        let (third, first) = (row(&list, 0), row(&list, 1));
        assert_eq!(laid_out_y(&third), 0.0, "laid out at its new place at once");
        assert_eq!(drawn_y(&third), 20.0, "and still drawn where it was");
        assert_eq!(drawn_y(&first), 0.0);

        let passes = layout_reactive::layout_passes();
        let start = Instant::now();
        tick(start);
        let mut now = start;
        for _ in 0..4 {
            now = frame(now, 25);
        }
        assert!(close(drawn_y(&third), 10.0), "halfway: {}", drawn_y(&third));
        assert!(close(drawn_y(&first), 5.0), "halfway: {}", drawn_y(&first));
        assert_eq!(
            layout_reactive::layout_passes(),
            passes,
            "only the transform moves while it plays"
        );

        frame(now, SLIDE_MS);
        assert_eq!(third.matrix_now(), IDENTITY, "and it lands on its layout");
        assert!(!motion_core::has_active());
    }

    #[test]
    fn a_change_mid_slide_carries_on_from_where_the_row_is_drawn() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items).animate_layout(slide());
        lay_out(&list);
        items.set(vec![3, 1, 2]);
        relayout_if_dirty();
        let third = row(&list, 0);
        let start = Instant::now();
        tick(start);
        let now = frame(start, SLIDE_MS / 2);
        let before = drawn_y(&third);
        assert!(close(before, 10.0), "{before}");

        items.set(vec![1, 2, 3]);
        relayout_if_dirty();
        assert_eq!(laid_out_y(&third), 20.0, "sent back");
        assert!(
            close(drawn_y(&third), before),
            "no jump at the interruption: {} then {}",
            before,
            drawn_y(&third)
        );

        let next = frame(now, 16);
        assert!(drawn_y(&third) > before, "moving on at once");
        frame(next, SLIDE_MS / 2 - 16);
        assert_eq!(
            third.matrix_now(),
            IDENTITY,
            "half the way back takes half the time"
        );
    }

    #[test]
    fn an_interrupted_spring_keeps_its_momentum() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items).animate_layout(Spring::gentle());
        lay_out(&list);
        items.set(vec![3, 1, 2]);
        relayout_if_dirty();
        let third = row(&list, 0);
        let start = Instant::now();
        tick(start);
        let mut now = start;
        for _ in 0..3 {
            now = frame(now, 16);
        }
        let before = drawn_y(&third);
        now = frame(now, 16);
        let moving = drawn_y(&third) - before;
        assert!(moving < 0.0, "sliding up");

        let at = drawn_y(&third);
        items.set(vec![1, 2, 3]);
        relayout_if_dirty();
        assert!(close(drawn_y(&third), at), "no jump at the interruption");
        frame(now, 16);
        assert!(
            drawn_y(&third) < at,
            "the spring still carries it up before turning back: {} then {}",
            at,
            drawn_y(&third)
        );
    }

    #[test]
    fn rows_after_a_leaving_row_close_its_gap_as_it_leaves() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items)
            .with_transition(fade())
            .animate_layout(slide());
        lay_out(&list);
        let height = || track_layout(list.layout_node()).unwrap().peek().height;
        assert_eq!(height(), 30.0);

        items.set(vec![1, 3]);
        relayout_if_dirty();
        let (leaving, last) = (row(&list, 1), row(&list, 2));
        assert!(leaving.is_leaving());
        assert_eq!(height(), 20.0, "the leaving row gives its space up at once");
        assert_eq!(laid_out_y(&last), 10.0);
        assert_eq!(
            drawn_y(&last),
            20.0,
            "while the row after it is still drawn below it"
        );
        assert_eq!(
            drawn_y(&leaving),
            10.0,
            "and the leaving row stays where it was"
        );

        let passes = layout_reactive::layout_passes();
        let start = Instant::now();
        tick(start);
        let now = frame(start, EXIT_MS / 2);
        assert!(
            close(drawn_y(&last), 15.0),
            "sliding up: {}",
            drawn_y(&last)
        );
        let (_, opacity) = leaving.mount().unwrap().frame_now();
        assert!(close(opacity, 0.5), "alongside the fade: {opacity}");
        assert_eq!(layout_reactive::layout_passes(), passes);

        frame(now, EXIT_MS / 2);
        assert_eq!(list.state.borrow().children.len(), 2, "gone after its exit");
        assert_eq!(drawn_y(&last), 10.0, "with the gap already closed");
    }

    #[test]
    fn without_animated_layout_a_leaving_row_keeps_its_space_to_the_end() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items).with_transition(fade());
        lay_out(&list);
        items.set(vec![1, 3]);
        relayout_if_dirty();
        let last = row(&list, 2);
        assert_eq!(laid_out_y(&last), 20.0);
        let start = Instant::now();
        tick(start);
        let now = frame(start, EXIT_MS / 2);
        assert_eq!(drawn_y(&last), 20.0);
        frame(now, EXIT_MS / 2);
        assert_eq!(drawn_y(&last), 10.0, "then jumps");
    }

    #[test]
    fn a_row_back_mid_exit_takes_its_space_back_smoothly() {
        fresh();
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items)
            .with_transition(fade())
            .animate_layout(slide());
        lay_out(&list);
        items.set(vec![1, 3]);
        relayout_if_dirty();
        let (returning, last) = (row(&list, 1), row(&list, 2));
        let start = Instant::now();
        tick(start);
        let now = frame(start, EXIT_MS / 2);
        let before = drawn_y(&last);

        items.set(vec![1, 2, 3]);
        relayout_if_dirty();
        assert!(!returning.is_leaving());
        assert_eq!(laid_out_y(&returning), 10.0, "back in the layout");
        assert_eq!(laid_out_y(&last), 20.0);
        assert!(
            close(drawn_y(&last), before),
            "no jump for the row after it"
        );
        frame(now, EXIT_MS);
        assert_eq!(drawn_y(&last), 20.0);
        assert_eq!(returning.matrix_now(), IDENTITY);
    }

    fn button(presses: Rc<Cell<u32>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(box_item(
            StyledContainer::new(
                LayoutStyle::new().width(40.0).height(40.0),
                |_| RectStyle::default(),
                vec![],
            )?
            .on_press(move || presses.set(presses.get() + 1)),
        ))
    }

    fn press(target: &mut dyn LayoutItem, x: f64, y: f64) {
        let (button, source) = (PointerButton::Primary, PointerSource::Mouse);
        target.on_event(&Event::PointerPressed {
            x,
            y,
            button,
            source: source.clone(),
        });
        target.on_event(&Event::PointerReleased {
            x,
            y,
            button,
            source,
        });
    }

    #[test]
    fn the_pointer_lands_on_a_sliding_row_where_it_is_drawn() {
        fresh();
        let items = signal(vec![1u32, 2]);
        let presses: Vec<Rc<Cell<u32>>> = (0..2).map(|_| Rc::new(Cell::new(0))).collect();
        let mut list = {
            let presses = presses.clone();
            ReactiveList::new(
                move || items.get(),
                |n: &u32| *n,
                move |n| button(Rc::clone(&presses[n as usize - 1])),
                0.0,
            )
            .unwrap()
            .animate_layout(slide())
        };
        lay_out(&list);
        items.set(vec![2, 1]);
        relayout_if_dirty();
        let second = row(&list, 0);
        assert_eq!(laid_out_y(&second), 0.0);
        assert_eq!(drawn_y(&second), 40.0);

        press(&mut list, 20.0, 60.0);
        assert_eq!(
            presses[1].get(),
            1,
            "the row drawn under the pointer takes it"
        );
        assert_eq!(presses[0].get(), 0, "not the one laid out there");
        assert!(crate::input_region::pointable(second.node(), 20.0, 60.0));
        assert!(!crate::input_region::pointable(second.node(), 20.0, 20.0));

        let start = Instant::now();
        tick(start);
        frame(start, SLIDE_MS);
        press(&mut list, 20.0, 20.0);
        assert_eq!(presses[1].get(), 2, "and follows it to where it lands");
    }

    #[test]
    fn a_zero_time_scale_lands_the_rows_at_once() {
        fresh();
        set_scale(0.0);
        let items = signal(vec![1u32, 2, 3]);
        let list = keyed_list(items)
            .with_transition(fade())
            .animate_layout(slide());
        lay_out(&list);
        items.set(vec![3, 1, 2]);
        relayout_if_dirty();
        let third = row(&list, 0);
        assert_eq!(third.matrix_now(), IDENTITY);
        assert_eq!(drawn_y(&third), 0.0);
        assert!(!motion_core::has_active(), "nothing left to play");
        set_scale(1.0);
    }

    #[test]
    fn a_list_freed_while_a_row_is_out_of_its_layout_frees_that_row_too() {
        fresh();
        let baseline = live_node_count();
        let items = signal(vec![1u32, 2, 3]);
        let scope = reactive_core::owner_scope();
        let owner = scope.id();
        let list = keyed_list(items)
            .with_transition(fade())
            .animate_layout(slide());
        drop(scope);
        lay_out(&list);
        items.set(vec![1, 3]);
        relayout_if_dirty();
        assert!(row(&list, 1).is_leaving());

        let node = list.layout_node();
        drop(list);
        reactive_core::dispose_owner(owner);
        crate::context::remove_node(node);
        assert_eq!(live_node_count(), baseline);
    }
}
