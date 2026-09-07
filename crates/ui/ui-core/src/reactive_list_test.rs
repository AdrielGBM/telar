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
