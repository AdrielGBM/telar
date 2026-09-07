use std::panic::catch_unwind;

use slotmap::Key;

use super::*;

/// The hazard versioned keys exist for, written down. The arena hands the freed slot straight back, so under a raw index the id that freed it addressed whatever moved in — and when that was the same type, `with_signal_value` downcast cleanly and returned the wrong signal's value with nothing to notice.
#[test]
fn the_id_that_freed_a_slot_cannot_read_what_moves_into_it() {
    let scope = crate::owner_scope();
    let owner = scope.id();
    let first = create_signal_storage(1i32);
    drop(scope);
    crate::dispose_owner(owner);
    let second = create_signal_storage(2i32);

    let slot = |key: SignalId| key.data().as_ffi() as u32;
    assert_eq!(slot(first), slot(second), "the slot is reused immediately");
    assert_ne!(first, second, "but the id that freed it is not");
    assert_eq!(with_signal_value::<i32, _>(second, |v| *v), 2);
    assert!(
        RUNTIME.with(|rt| !rt.borrow().signals.contains_key(first)),
        "and the stale id resolves to nothing, though its slot is occupied"
    );
}

/// The policy in the module doc, as three assertions. A dead read is an error, a dead write is an error outside a teardown, and the messages say which of the two happened — the one message for both used to send a reader looking for a call that was never made.
#[test]
fn a_dead_handle_reads_and_writes_differently() {
    let scope = crate::owner_scope();
    let owner = scope.id();
    let id = create_signal_storage(7i32);
    drop(scope);
    crate::dispose_owner(owner);

    let read = catch_unwind(|| with_signal_value::<i32, _>(id, |v| *v)).unwrap_err();
    let written = catch_unwind(|| set_signal_value(id, 8i32)).unwrap_err();

    let message =
        |e: &Box<dyn std::any::Any + Send>| e.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        message(&read).contains("i32 read after"),
        "{}",
        message(&read)
    );
    assert!(
        message(&written).contains("i32 written after"),
        "{}",
        message(&written)
    );
}

/// A cleanup writing a signal a sibling root already freed is the shape of a teardown, not a mistake in it: `dispose_surface_owners` frees roots in arena order, and the surface root owns everything created outside a scope.
#[test]
fn a_write_during_a_teardown_is_tolerated() {
    let scope = crate::owner_scope();
    let owner = scope.id();
    let id = create_signal_storage(1i32);
    drop(scope);
    crate::dispose_owner(owner);

    RUNTIME.with(|rt| rt.borrow_mut().disposing += 1);
    set_signal_value(id, 2i32);
    update_signal_value::<i32>(id, |v| *v += 1);
    RUNTIME.with(|rt| rt.borrow_mut().disposing -= 1);
}

/// What a handle kept past its owner is supposed to do instead of crashing.
#[test]
fn a_handle_can_ask_whether_its_storage_is_there() {
    let live = crate::signal(1i32);
    assert!(live.is_alive(), "a signal whose owner is alive is alive");
    assert_eq!(live.try_get(), Some(1));

    let scope = crate::owner_scope();
    let owner = scope.id();
    let doomed = crate::signal(2i32);
    drop(scope);
    crate::dispose_owner(owner);

    assert!(
        !doomed.is_alive(),
        "and disposing the owner kills its storage"
    );
    assert_eq!(doomed.try_get(), None);
    assert_eq!(doomed.try_with(|v| *v), None);
}
