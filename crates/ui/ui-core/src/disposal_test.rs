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
