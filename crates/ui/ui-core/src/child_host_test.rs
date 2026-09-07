use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime, track_layout};
use layout_core::{AvailableSpace, LayoutStyle};
use reactive_core::signal;
use ui_tree::Component;

fn leaf10() -> Box<dyn LayoutItem> {
    Box::new(Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap())
}

// A 10×10 leaf with its layout node, so a test can read the laid-out rect.
fn leaf10_node() -> (NodeId, Box<dyn LayoutItem>) {
    let c = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    (c.layout_node(), Box::new(c))
}

fn group_len(node: &RenderNode) -> usize {
    match node {
        RenderNode::Group { children, .. } => children.len(),
        _ => panic!("expected Group"),
    }
}

#[test]
fn fragment_children_flatten_and_reconcile() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2, 3]);
    let src = items;
    let container = Container::from_slots(
        LayoutStyle::new().flex_row(),
        vec![
            ChildSlot::stat(leaf10()),
            fragment(move || src.get(), |n: &u32| *n, |_n| Ok(leaf10()), 0.0),
            ChildSlot::stat(leaf10()),
        ],
    )
    .unwrap();

    assert_eq!(group_len(&container.view()), 5, "2 static + 3 dynamic");

    items.set(vec![9]);
    assert_eq!(group_len(&container.view()), 3, "2 static + 1 dynamic");

    items.set(vec![9, 8, 7, 6]);
    assert_eq!(group_len(&container.view()), 6, "2 static + 4 dynamic");
}

/// A fragment whose host node is gone must go quiet, not take the process down.
///
/// The real shape of this is a reactive branch holding a reactive list — `if $has_active { for p in $params }` — where one write changes both: the branch tears its content down (freeing the very node the fragment reconciles into, since a fragment host *is* its container's node) and the list's own effect still has a run scheduled. The effect outlives the node because a `Segment` holds the widget so a re-render mid-dispatch can still flatten it, so this is not a lifetime that can simply be tightened. Clearing the selection in a properties panel did exactly this.
#[test]
fn a_fragment_whose_node_is_gone_reconciles_into_nothing() {
    reset_layout_runtime();
    let items = signal(vec![1u32, 2, 3]);
    let src = items;
    let container = Container::from_slots(
        LayoutStyle::new().flex_row(),
        vec![fragment(
            move || src.get(),
            |n: &u32| *n,
            |_n| Ok(leaf10()),
            0.0,
        )],
    )
    .unwrap();
    assert_eq!(group_len(&container.view()), 3);

    // What the branch teardown does to it, without the branch.
    crate::context::remove_node(container.layout_node());
    items.set(vec![9]);
    items.set(Vec::new());
}

// The fragment's items lay out as real siblings of the statics, in the host's row direction and between them, rather than stacked in a private column.
#[test]
fn fragment_items_flow_in_host_direction_between_statics() {
    reset_layout_runtime();
    let (s0, static0) = leaf10_node();
    let (s1, static1) = leaf10_node();
    let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = built.clone();
    let items = signal(vec![1u32, 2, 3]);
    let src = items;

    let container = Container::from_slots(
        LayoutStyle::new().flex_row(),
        vec![
            ChildSlot::stat(static0),
            fragment(
                move || src.get(),
                |n: &u32| *n,
                move |_n| {
                    let (node, item) = leaf10_node();
                    sink.borrow_mut().push(node);
                    Ok(item)
                },
                0.0,
            ),
            ChildSlot::stat(static1),
        ],
    )
    .unwrap();

    compute_layout(
        container.layout_node(),
        AvailableSpace::Definite(500.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();

    let frag = built.borrow().clone();
    assert_eq!(frag.len(), 3);
    let x = |node: NodeId| track_layout(node).unwrap().get().x;
    // static0 → frag[0..3] → static1, strictly left-to-right: horizontal and interleaved.
    let xs = [x(s0), x(frag[0]), x(frag[1]), x(frag[2]), x(s1)];
    for pair in xs.windows(2) {
        assert!(
            pair[1] > pair[0],
            "children must advance along the row (got x sequence {xs:?})"
        );
    }
    let y = |node: NodeId| track_layout(node).unwrap().get().y;
    assert!(
        (y(frag[0]) - y(s0)).abs() < 0.01 && (y(s1) - y(frag[2])).abs() < 0.01,
        "the fragment's items must sit between the statics that bracket them"
    );
}

// Chips built when a socket answers, well after the bar first laid out, must still fire their handler. A `from_slots` row is the component root, hosting a `fragment_gap` whose body is a bare `StyledContainer`.
#[test]
fn pressing_a_fragment_chip_added_after_layout_fires_its_handler() {
    use crate::context::{new_container, relayout_if_dirty};
    use crate::styled_container::StyledContainer;
    use layout_core::{AlignItems, JustifyContent};
    use platform_core::{Event, PointerButton, PointerSource};
    use renderer_core::RectStyle;

    reset_layout_runtime();
    let fired = Rc::new(std::cell::Cell::new(0i32));
    let ids = signal(Vec::<i32>::new()); // empty at first, like the snapshot before the socket answers
    let src = ids;
    let sink = fired.clone();

    let mut row = Container::from_slots(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER),
        vec![fragment(
            move || src.get(),
            |id: &i32| *id as u64,
            move |id| {
                let sink = sink.clone();
                let chip = StyledContainer::new(
                    LayoutStyle::new()
                        .flex_column()
                        .width(24.0)
                        .height(24.0)
                        .align_items(AlignItems::CENTER)
                        .justify_content(JustifyContent::CENTER),
                    move |_| RectStyle::default(),
                    vec![],
                )?
                .on_press(move || sink.set(id));
                Ok(Box::new(chip) as Box<dyn LayoutItem>)
            },
            8.0,
        )],
    )
    .unwrap();

    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(24.0),
        &[row.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(24.0),
    )
    .unwrap();

    ids.set(vec![7, 8, 9]);
    relayout_if_dirty();

    let press = |x: f64, y: f64| Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    };
    let release = |x: f64, y: f64| Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    };
    row.on_event(&press(12.0, 12.0));
    row.on_event(&release(12.0, 12.0));

    assert_eq!(
        fired.get(),
        7,
        "clicking the first workspace-style chip should fire its on_press"
    );
}

// Regression: a `from_slots` stretch row of bare `StyledContainer` chips in a stretch zone must stretch each chip to the full zone height, with no injected flex-column trapping them at content height.
#[test]
fn stretch_row_of_fragment_chips_fills_the_zone_height() {
    use crate::context::{new_container, relayout_if_dirty};
    use crate::styled_container::StyledContainer;
    use layout_core::AlignItems;
    use renderer_core::RectStyle;

    reset_layout_runtime();
    let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = built.clone();
    let ids = signal(Vec::<i32>::new());
    let src = ids;

    let row = Container::from_slots(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::STRETCH),
        vec![fragment(
            move || src.get(),
            |id: &i32| *id as u64,
            move |_id| {
                let sink = sink.clone();
                let chip = StyledContainer::new(
                    LayoutStyle::new().flex_column().padding_horizontal(10.0),
                    move |_| RectStyle::default(),
                    vec![Box::new(Container::new(
                        LayoutStyle::new().width(10.0).height(13.0),
                        vec![],
                    )?)],
                )?;
                let node = chip.layout_node();
                sink.borrow_mut().push(node);
                Ok(Box::new(chip) as Box<dyn LayoutItem>)
            },
            8.0,
        )],
    )
    .unwrap();

    let root = new_container(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::STRETCH)
            .width(200.0)
            .height(34.0),
        &[row.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(34.0),
    )
    .unwrap();

    ids.set(vec![1, 2, 3]);
    relayout_if_dirty();

    let chips = built.borrow().clone();
    assert_eq!(chips.len(), 3, "three chips built");
    for node in chips {
        let h = track_layout(node).unwrap().get().height;
        assert!(
            (h - 34.0).abs() < 0.01,
            "each chip must stretch to the 34px zone height, got {h} (content height ~13 means a \
             collapsing wrapper crept back in)"
        );
    }
}

// Transparent and spaced: 10px leaves plus an 8px per-item leading margin give x 0, 18, 36.
#[test]
fn fragment_gap_spaces_items_along_the_row() {
    reset_layout_runtime();
    let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = built.clone();
    let items = signal(vec![1u32, 2, 3]);
    let src = items;
    let container = Container::from_slots(
        LayoutStyle::new().flex_row(),
        vec![fragment(
            move || src.get(),
            |n: &u32| *n,
            move |_n| {
                let (node, item) = leaf10_node();
                sink.borrow_mut().push(node);
                Ok(item)
            },
            8.0,
        )],
    )
    .unwrap();
    compute_layout(
        container.layout_node(),
        AvailableSpace::Definite(500.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();

    let frag = built.borrow().clone();
    assert_eq!(frag.len(), 3);
    let x = |node: NodeId| track_layout(node).unwrap().get().x;
    assert!(x(frag[0]).abs() < 0.01, "first item flush: {}", x(frag[0]));
    assert!(
        (x(frag[1]) - 18.0).abs() < 0.01,
        "10px item + 8px gap → 18: {}",
        x(frag[1])
    );
    assert!(
        (x(frag[2]) - 36.0).abs() < 0.01,
        "two 10px items + two 8px gaps → 36: {}",
        x(frag[2])
    );
}

// Re-applied per reconcile, not baked at build: a keyed item moving to the front must lose its leading margin, and the former first must gain one.
#[test]
fn fragment_gap_reorder_moves_gap_off_the_new_first_item() {
    reset_layout_runtime();
    let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = built.clone();
    let items = signal(vec![1u32, 2, 3]);
    let src = items;
    let container = Container::from_slots(
        LayoutStyle::new().flex_row(),
        vec![fragment(
            move || src.get(),
            |n: &u32| *n,
            move |_n| {
                let (node, item) = leaf10_node();
                sink.borrow_mut().push(node);
                Ok(item)
            },
            8.0,
        )],
    )
    .unwrap();
    let root = container.layout_node();
    let space = || {
        (
            AvailableSpace::Definite(500.0),
            AvailableSpace::Definite(50.0),
        )
    };
    compute_layout(root, space().0, space().1).unwrap();

    let (n1, n3) = {
        let b = built.borrow();
        (b[0], b[2])
    };
    let x = |node: NodeId| track_layout(node).unwrap().get().x;
    assert!(x(n1).abs() < 0.01, "key1 first, flush: {}", x(n1));
    assert!((x(n3) - 36.0).abs() < 0.01, "key3 last, at 36: {}", x(n3));

    items.set(vec![3, 1, 2]);
    compute_layout(root, space().0, space().1).unwrap();
    assert!(
        x(n3).abs() < 0.01,
        "reordered-to-front item drops its gap margin: {}",
        x(n3)
    );
    assert!(
        (x(n1) - 18.0).abs() < 0.01,
        "former-first item now second, at 18: {}",
        x(n1)
    );
}
