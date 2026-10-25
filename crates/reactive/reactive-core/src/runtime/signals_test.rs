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

/// The loop that four separate call sites in one downstream project had a paragraph warning about: a signal read inside an effect that later writes it. The version moves on every write, so the effect is scheduled by its own write and arrives back here for ever.
#[cfg(debug_assertions)]
#[test]
fn an_effect_writing_what_it_tracked_is_reported_once() {
    let scope = crate::owner_scope();
    let signal = crate::signal(0i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0u32));

    let counted = seen.clone();
    crate::effect(move || {
        // Tracked on purpose, which is the mistake being reported.
        let held = signal.get();
        if counted.get() < 3 {
            counted.set(counted.get() + 1);
            signal.set(held + 1);
        }
    });
    flush();

    assert!(
        seen.get() > 1,
        "the effect did wake itself, which is the loop"
    );
    let reported = RUNTIME.with(|rt| rt.borrow().self_waking.len());
    assert_eq!(reported, 1, "and it is reported once, not once per pass");
    drop(scope);
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

/// What `.rsx` writes for `$count += $step.get()`, and the push of a fresh id: the closure given to `.update` reads another signal.
#[test]
fn an_update_closure_can_read_another_signal() {
    let scope = crate::owner_scope();
    let count = crate::signal(1i32);
    let step = crate::signal(2i32);
    count.update(|__v| *__v += step.get());
    assert_eq!(count.get(), 3);

    let items = crate::signal(Vec::<u32>::new());
    let next_id = crate::signal(7u32);
    crate::effect(move || items.update(|v| v.push(next_id.get())));
    next_id.set(8);
    assert_eq!(
        items.get(),
        [7, 8],
        "the read inside the closure is tracked by the effect running it"
    );
    drop(scope);
}

#[test]
fn a_with_closure_can_read_another_signal() {
    let scope = crate::owner_scope();
    let a = crate::signal(2i32);
    let b = crate::signal(3i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));

    let sum = a.with(|x| b.with(|y| x + y));
    assert_eq!(sum, 5);

    let into = seen.clone();
    crate::effect(move || into.set(a.with(|x| x * b.get())));
    b.set(10);
    assert_eq!(seen.get(), 20);
    drop(scope);
}

/// Two shared borrows of one value alias nothing, so a read nested in the same signal's `.with` is allowed.
#[test]
fn a_signal_can_be_read_again_inside_its_own_with() {
    let scope = crate::owner_scope();
    let a = crate::signal(4i32);
    assert_eq!(a.with(|outer| outer + a.get()), 8);
    drop(scope);
}

#[test]
fn touching_a_signal_inside_its_own_update_panics_naming_it() {
    let scope = crate::owner_scope();
    let count = crate::signal(0i32);

    let written = catch_unwind(|| count.update(|_| count.set(1))).unwrap_err();
    let message = panic_message(&written);
    assert!(
        message.contains("signal of type i32 written inside its own `.with` / `.update`"),
        "{message}"
    );
    assert!(
        message.contains("signals_test.rs"),
        "both call sites are the caller's, not the runtime's: {message}"
    );

    let read = catch_unwind(|| count.update(|v| *v = count.get())).unwrap_err();
    assert!(
        panic_message(&read).contains("signal of type i32 read inside its own"),
        "{}",
        panic_message(&read)
    );

    let written_in_with = catch_unwind(|| count.with(|_| count.set(2))).unwrap_err();
    assert!(
        panic_message(&written_in_with).contains("i32 written inside its own"),
        "{}",
        panic_message(&written_in_with)
    );

    count.set(5);
    assert_eq!(count.get(), 5, "the unwind released the signal's borrow");
    drop(scope);
}

/// The update holds a batch open, so an effect reading the signal being updated runs once, after the closure, and sees both writes.
#[test]
fn a_write_inside_update_wakes_its_effects_after_the_closure_returns() {
    let scope = crate::owner_scope();
    let items = crate::signal(Vec::<i32>::new());
    let total = crate::signal(0i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let runs = std::rc::Rc::new(std::cell::Cell::new(0u32));

    let (into, counted) = (seen.clone(), runs.clone());
    crate::effect(move || {
        counted.set(counted.get() + 1);
        into.set(items.with(Vec::len) + total.get() as usize);
    });

    items.update(|v| {
        v.push(1);
        total.set(2);
        assert_eq!(seen.get(), 0, "nothing ran while the closure was running");
    });
    assert_eq!(seen.get(), 3);
    assert_eq!(
        runs.get(),
        2,
        "one run for the update and the write inside it"
    );
    drop(scope);
}

#[test]
fn an_update_inside_a_batch_flushes_when_the_batch_closes() {
    let scope = crate::owner_scope();
    let count = crate::signal(0i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));

    let into = seen.clone();
    crate::effect(move || into.set(count.get()));
    crate::batch(|| {
        count.update(|v| *v += 1);
        assert_eq!(seen.get(), 0, "the outer batch still holds the flush");
        count.update(|v| *v += 1);
    });
    assert_eq!(seen.get(), 2);
    drop(scope);
}

#[test]
fn an_update_that_panics_leaves_later_writes_flushing() {
    let scope = crate::owner_scope();
    let count = crate::signal(0i32);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));

    let into = seen.clone();
    crate::effect(move || into.set(count.get()));
    catch_unwind(|| count.update(|_| panic!("in the closure"))).unwrap_err();
    count.set(4);
    assert_eq!(
        seen.get(),
        4,
        "the batch the update opened was closed by the unwind"
    );
    drop(scope);
}

/// The effect a write inside `.with` wakes writes the signal being read, which would collide if it ran before the closure returned.
#[test]
fn a_write_inside_with_wakes_its_effects_after_the_closure_returns() {
    let scope = crate::owner_scope();
    let a = crate::signal(0i32);
    let b = crate::signal(0i32);
    crate::effect(move || {
        let next = b.get();
        if next > 0 {
            a.set(next * 10);
        }
    });

    let seen_inside = a.with(|held| {
        b.set(2);
        *held
    });
    assert_eq!(seen_inside, 0, "the closure saw the value it was lent");
    assert_eq!(a.get(), 20, "and the effect ran once it had returned");
    a.read_only().with(|_| b.set(3));
    assert_eq!(a.get(), 30, "a read handle's `.with` holds the batch too");
    drop(scope);
}

#[test]
fn a_collision_names_the_outer_borrow_not_a_nested_read() {
    let scope = crate::owner_scope();
    let count = crate::signal(0i32);

    let outer_line = line!() + 2;
    let written = catch_unwind(|| {
        count.with(|_| {
            let _nested = count.get();
            count.set(1);
        })
    })
    .unwrap_err();
    let message = panic_message(&written);
    assert!(
        message.contains(&format!("last taken: {}:{outer_line}:", file!())),
        "the borrow still held is the outer `.with`: {message}"
    );
    drop(scope);
}
