use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::*;
use crate::runtime::signals::create_signal_storage;
use crate::{effect, memo, signal};

fn alive(id: SignalId) -> bool {
    RUNTIME.with(|rt| rt.borrow().signals.contains_key(id))
}

#[test]
fn disposing_an_owner_takes_its_children_with_it() {
    let outer = owner_scope();
    let held_by_outer = create_signal_storage(1i32);
    let held_by_inner = {
        let _inner = owner_scope();
        create_signal_storage(2i32)
    };

    let root = outer.id();
    drop(outer);
    dispose_owner(root);

    assert!(
        !alive(held_by_outer),
        "disposing an owner disposes what it owns"
    );
    assert!(!alive(held_by_inner), "a child owner is disposed with it");
}

#[test]
fn an_effect_stops_running_when_its_owner_is_disposed() {
    let count = signal(0i32);
    let read = count.read_only();
    let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));

    let scope = owner_scope();
    let seen_c = Rc::clone(&seen);
    effect(move || seen_c.borrow_mut().push(read.get()));
    let owner = scope.id();
    drop(scope);

    count.set(1);
    assert_eq!(*seen.borrow(), vec![0, 1]);

    dispose_owner(owner);
    count.set(2);
    assert_eq!(*seen.borrow(), vec![0, 1], "the owner deregistered it");
}

/// T-1.3's guarantee, for the owner stack. A panic mid-build that leaves the stack deeper than it started puts every later creation under the wrong owner — and a lifetime error surfaces nowhere near the panic that caused it.
#[test]
fn a_panic_mid_build_leaves_the_stack_where_it_found_it() {
    let before = current_owner();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let _scope = owner_scope();
        let _nested = owner_scope();
        panic!("boom");
    }));

    assert!(
        outcome.is_err(),
        "the panic is reported rather than swallowed"
    );
    assert_eq!(current_owner(), before);

    let scope = owner_scope();
    let after_panic = create_signal_storage(3i32);
    let owner = scope.id();
    drop(scope);
    dispose_owner(owner);
    assert!(!alive(after_panic), "and the tree is usable afterwards");
}

#[test]
fn a_surface_disposes_the_owner_roots_stamped_with_it() {
    let surface = SurfaceHandle(41);
    // `SurfaceHandle::enter` delegates to a hook a higher crate installs, and with none installed it leaves the current surface alone, so the stamp has to be set through the primitive the hook itself calls.
    let held = {
        let previous = crate::set_current_surface(surface);
        let scope = owner_scope();
        let held = create_signal_storage(4i32);
        drop(scope);
        crate::set_current_surface(previous);
        held
    };
    let elsewhere = {
        let scope = owner_scope();
        let elsewhere = create_signal_storage(5i32);
        drop(scope);
        elsewhere
    };

    dispose_surface_owners(surface);

    assert!(
        !alive(held),
        "disposing a surface takes the roots stamped with it"
    );
    assert!(alive(elsewhere), "another surface's owners are untouched");
}

/// The allocation shape the frame budget guards. An owner that holds nothing must cost its struct and no more, or a list render turns into thousands of allocations for owners that never hold anything.
#[test]
fn an_owner_that_holds_nothing_allocates_nothing() {
    let scope = owner_scope();
    let owner = scope.id();
    drop(scope);

    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let entry = &rt.owners[owner];
        assert_eq!(entry.children.capacity(), 0);
        assert_eq!(entry.signals.capacity(), 0);
        assert_eq!(entry.effects.capacity(), 0);
    });
    dispose_owner(owner);
}

/// A teardown must not run effects over what it has already freed.
///
/// `dispose_surface_owners` frees one root at a time, and a root's cleanups run inside a batch that ends in a flush. An effect that is still registered — its own root's turn has not come yet — re-runs in that flush and reads a memo the previous root already freed, which is a panic in the middle of a window closing rather than an error anyone can act on.
#[test]
fn a_teardown_does_not_run_effects_over_what_it_already_freed() {
    let ticks = signal(0i32);
    let read = ticks.read_only();

    let holder = owner_scope();
    let title = memo(move || read.get().to_string());
    let holder_id = holder.id();
    drop(holder);

    let reader = owner_scope();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = Rc::clone(&seen);
    effect(move || {
        let _ = read.get();
        seen_c.borrow_mut().push(title.get());
    });
    on_cleanup(move || ticks.set(1));
    let reader_id = reader.id();
    drop(reader);

    assert_eq!(*seen.borrow(), vec!["0".to_string()]);

    dispose_owner(holder_id);
    dispose_owner(reader_id);

    assert_eq!(
        *seen.borrow(),
        vec!["0".to_string()],
        "the effect did not run again over a memo that was already gone"
    );
}

/// The same teardown, through the door a closing window actually uses. `dispose_surface_owners` frees the surface's roots one after another, and the guard has to span the set rather than each root: between two of them is exactly where the flush used to land.
#[test]
fn a_surface_teardown_does_not_run_effects_over_what_it_already_freed() {
    let ticks = crate::detached(|| signal(0i32));
    let read = ticks.read_only();

    let holder = owner_scope();
    let title = memo(move || read.get().to_string());
    drop(holder);

    let reader = owner_scope();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = Rc::clone(&seen);
    effect(move || {
        let _ = read.get();
        seen_c.borrow_mut().push(title.get());
    });
    on_cleanup(move || ticks.set(1));
    drop(reader);

    dispose_surface_owners(current_surface());

    assert_eq!(*seen.borrow(), vec!["0".to_string()]);
}
