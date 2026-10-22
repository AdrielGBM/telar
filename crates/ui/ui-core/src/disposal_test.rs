use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use layout_reactive::{LayoutStyle, live_node_count, new_container};
use reactive_core::{on_cleanup, owner_scope};

use super::*;

fn quietly<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    outcome
}

fn retire_with(cleanup: impl FnOnce() + 'static) {
    let scope = owner_scope();
    on_cleanup(cleanup);
    let owner = scope.id();
    drop(scope);
    let node = new_container(LayoutStyle::new(), &[]).unwrap();
    retire(Some(owner), node);
}

fn message(payload: &PanicPayload) -> &str {
    payload.downcast_ref::<&str>().copied().unwrap_or_default()
}

#[test]
fn a_panicking_teardown_still_frees_the_rest_of_the_batch() {
    let nodes = live_node_count();
    let ran = Rc::new(Cell::new(false));
    let seen = Rc::clone(&ran);

    let outcome = quietly(|| {
        let _walk = dispatching();
        retire_with(|| panic!("cleanup"));
        retire_with(move || seen.set(true));
    });

    let payload = outcome.expect_err("the panic still reaches the dispatch");
    assert_eq!(message(&payload), "cleanup");
    assert!(ran.get(), "the child retired after it was freed too");
    assert_eq!(live_node_count(), nodes, "both nodes were removed");
}

/// Resuming a teardown's panic from a guard dropped by another unwind is a panic out of a `Drop` during a panic, which aborts the process rather than failing the test.
#[test]
fn a_panicking_teardown_during_an_unwind_leaves_that_unwind_alone() {
    let nodes = live_node_count();

    let outcome = quietly(|| {
        let _walk = dispatching();
        retire_with(|| panic!("cleanup"));
        panic!("handler");
    });

    let payload = outcome.expect_err("the handler panicked");
    assert_eq!(message(&payload), "handler");
    assert_eq!(
        live_node_count(),
        nodes,
        "the retired node was still removed"
    );
}

/// Disposing a tree's owner frees only its reactive half: its nodes stayed laid out with their rect signals freed, and its root stayed the overlay host the next tree's overlays attached to.
#[test]
fn dropping_an_owned_build_frees_its_nodes_and_lets_go_of_the_host_its_root_was() {
    use crate::LayoutItem;
    use crate::context::{compute_layout, relayout_if_dirty, reset_layout_runtime};
    use layout_core::AvailableSpace;
    use layout_reactive::overlay_viewport;

    reset_layout_runtime();
    let baseline = live_node_count();
    let (root, built) = OwnedBuild::run(|| {
        let panel =
            crate::Container::new(LayoutStyle::new().width(50.0).height(20.0), vec![]).unwrap();
        let overlay =
            crate::overlay::Overlay::new(LayoutStyle::new(), vec![Box::new(panel)]).unwrap();
        crate::Container::new(LayoutStyle::new().flex_column(), vec![Box::new(overlay)]).unwrap()
    });
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    relayout_if_dirty();
    assert!(overlay_viewport().is_some(), "its root hosts the overlays");

    drop(built);
    assert_eq!(live_node_count(), baseline, "every node it built is freed");
    assert_eq!(overlay_viewport(), None, "and its root hosts nothing");
    relayout_if_dirty();
    drop(root);
}

#[test]
fn an_owned_build_dropped_after_a_reset_frees_nothing_of_the_next_tree() {
    use crate::context::reset_layout_runtime;

    reset_layout_runtime();
    let (_, stale) = OwnedBuild::run(|| {
        crate::Container::new(LayoutStyle::new(), vec![]).unwrap();
    });

    reset_layout_runtime();
    let (kept, fresh) =
        OwnedBuild::run(|| crate::Container::new(LayoutStyle::new(), vec![]).unwrap());
    let nodes = live_node_count();
    drop(stale);

    assert_eq!(live_node_count(), nodes, "the stale build freed no node");
    assert!(
        layout_reactive::track_layout(crate::LayoutItem::layout_node(&kept)).is_some(),
        "the next tree's node is still registered"
    );
    drop(fresh);
    assert_eq!(live_node_count(), 0, "its own build still frees it");
}
