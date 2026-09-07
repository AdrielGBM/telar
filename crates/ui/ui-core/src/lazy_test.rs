use std::cell::Cell;

use layout_core::AvailableSpace;
use reactive_core::signal;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};

fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(10.0).height(10.0),
        vec![],
    )?))
}

#[test]
fn defers_construction_until_the_condition_first_holds() {
    reset_layout_runtime();
    let show = signal(false);
    let builds = Rc::new(Cell::new(0));
    let lazy = {
        let (cond, builds) = (show, builds.clone());
        Lazy::new(
            LayoutStyle::new().flex_column(),
            move || cond.get(),
            move || {
                builds.set(builds.get() + 1);
                Ok(vec![leaf()?])
            },
        )
        .unwrap()
    };
    assert_eq!(builds.get(), 0, "a block never shown costs nothing");
    assert!(
        !lazy.is_built(),
        "nothing is built before the condition first holds"
    );
    assert!(
        matches!(lazy.view(), RenderNode::Empty),
        "so there is nothing to draw"
    );

    show.set(true);
    assert_eq!(builds.get(), 1, "the first showing builds the subtree");
    assert!(lazy.is_built(), "the condition holding is what builds it");
}

/// The difference from a reactive `if`: toggling off and on again shows the *same* subtree rather than disposing and rebuilding it, so anything it held is still there.
#[test]
fn builds_once_and_only_toggles_afterwards() {
    reset_layout_runtime();
    let show = signal(true);
    let builds = Rc::new(Cell::new(0));
    let lazy = {
        let (cond, builds) = (show, builds.clone());
        Lazy::new(
            LayoutStyle::new().flex_column(),
            move || cond.get(),
            move || {
                builds.set(builds.get() + 1);
                Ok(vec![leaf()?])
            },
        )
        .unwrap()
    };
    assert_eq!(builds.get(), 1, "an initially-true block builds at once");
    let node = lazy.state.borrow().children[0].node();

    show.set(false);
    show.set(true);
    show.set(false);
    show.set(true);
    assert_eq!(builds.get(), 1, "reopening never rebuilds");
    assert_eq!(
        lazy.state.borrow().children[0].node(),
        node,
        "it is the same subtree, not a fresh one"
    );
}

#[test]
fn a_hidden_block_takes_no_space() {
    reset_layout_runtime();
    let show = signal(true);
    let lazy = {
        let cond = show;
        Lazy::new(
            LayoutStyle::new().flex_column(),
            move || cond.get(),
            || Ok(vec![leaf()?]),
        )
        .unwrap()
    };
    let node = lazy.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert!(
        track_layout(node).unwrap().get().height > 0.0,
        "a hidden block still reserves its own height"
    );

    show.set(false);
    compute_layout(
        node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(track_layout(node).unwrap().get().height, 0.0);
}

#[test]
fn a_hidden_block_ignores_events() {
    reset_layout_runtime();
    let show = signal(false);
    let mut lazy = {
        let cond = show;
        Lazy::new(
            LayoutStyle::new().flex_column(),
            move || cond.get(),
            || Ok(vec![leaf()?]),
        )
        .unwrap()
    };
    assert_eq!(
        lazy.on_event(&Event::CursorEntered),
        EventResult::Ignored,
        "an unbuilt block answers for nothing"
    );
}
