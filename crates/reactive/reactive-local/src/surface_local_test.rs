crate::surface_local! {
    slot PROBE: u32 = 0;
    access with_probe, with_probe_ref;
    context ProbeContext;
}

static WORLDS_DROPPED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct World;

impl Drop for World {
    fn drop(&mut self) {
        WORLDS_DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

crate::surface_local! {
    slot WORLD: World = World;
    access with_world, with_world_ref;
    context WorldContext;
}

fn worlds_dropped() -> u32 {
    WORLDS_DROPPED.load(std::sync::atomic::Ordering::Relaxed)
}

fn probe() -> u32 {
    with_probe_ref(|value| *value)
}

fn set_probe(value: u32) {
    with_probe(|probe| *probe = value);
}

fn unwind() {
    panic!("unwinds through the entry");
}

fn quietly<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(quiet);
    outcome
}

/// The ambient world is made the first time it is reached and freed when its thread is released, and reaching it again makes a fresh one that is freed in turn: a guest's host releases its thread before unloading it, and nothing of a build may stay behind.
#[test]
fn the_ambient_world_is_freed_when_its_thread_is_released_and_made_again_when_reached() {
    std::thread::spawn(|| {
        let before = worlds_dropped();
        with_world_ref(|_| ());
        crate::thread_exit::release_thread();
        assert_eq!(worlds_dropped(), before + 1);
        with_world(|_| ());
        crate::thread_exit::release_thread();
        assert_eq!(worlds_dropped(), before + 2);
    })
    .join()
    .unwrap();
}

/// An entry holds a pointer it restores, which may be the ambient world: freeing it then would leave the slot pointing at freed memory once the entry ends.
#[test]
fn the_ambient_world_is_kept_while_an_entry_is_under_way_and_released_after() {
    std::thread::spawn(|| {
        let before = worlds_dropped();
        let surface = WorldContext::new();
        surface.with_entered(|| {
            WorldContext::with_ambient_entered(|| {
                with_world_ref(|_| ());
                crate::thread_exit::release_thread();
            });
        });
        assert_eq!(worlds_dropped(), before);
        drop(surface);
        assert_eq!(
            worlds_dropped(),
            before + 1,
            "the surface's own world went with its context"
        );
    })
    .join()
    .unwrap();
}

/// The shape every `surface_local!` world has hit at least once, and the reason six of them carry a hand-written "copy out, drop the borrow, write after" dance: a closure holding the slot reaches back into it. The panic now points at the two lines instead of at the `RefCell` that noticed.
#[test]
fn a_reentrant_slot_borrow_names_both_call_sites() {
    let outcome = quietly(|| with_probe(|_| with_probe(|_| ())));

    let payload = outcome.expect_err("the inner borrow cannot succeed");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(message.contains("`PROBE` is already borrowed"), "{message}");
    assert_eq!(
        message.matches(file!()).count(),
        2,
        "both sites are named, and `#[track_caller]` put them in the caller's file rather than in the macro:\n{message}"
    );
}

/// And the case that must keep working: two surfaces are two worlds, so reaching the second from inside the first is not reentrancy at all. The recorded site is per slot rather than per instance, which is why this is worth pinning — the check itself has to stay on the `RefCell`, not on the record.
#[test]
fn entering_another_surface_is_not_a_collision() {
    let other = ProbeContext::new();
    with_probe(|outer| {
        *outer = 1;
        other.with_entered(|| {
            with_probe(|inner| {
                assert_eq!(*inner, 0, "the second surface has its own value");
                *inner = 2;
            });
        });
    });
    with_probe_ref(|ambient| assert_eq!(*ambient, 1, "and the first kept its own"));
}

/// A shared borrow blocks a mutable one just the same, and that pairing is the one a reader would least expect to be a collision at all.
#[test]
fn a_read_that_blocks_a_write_is_reported_too() {
    let outcome = quietly(|| with_probe_ref(|_| with_probe(|_| ())));

    let payload = outcome.expect_err("a write cannot join a live read");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(message.contains("`PROBE` is already borrowed"), "{message}");
}

#[test]
fn nested_entries_each_see_their_own_world_and_restore_the_one_around_them() {
    let first = ProbeContext::new();
    let second = ProbeContext::new();
    set_probe(10);
    first.with_entered(|| {
        set_probe(1);
        second.with_entered(|| {
            assert_eq!(probe(), 0);
            set_probe(2);
            first.with_entered(|| assert_eq!(probe(), 1, "re-entering the outer surface"));
            assert_eq!(probe(), 2);
        });
        assert_eq!(probe(), 1);
    });
    assert_eq!(probe(), 10);
    second.with_entered(|| assert_eq!(probe(), 2));
}

#[test]
fn an_entry_hands_back_what_its_closure_returns() {
    let surface = ProbeContext::new();
    surface.with_entered(|| set_probe(7));
    assert_eq!(surface.with_entered(probe), 7);
}

/// A panic inside an entry unwinds through it: the slot must be pointing back at the world around it once the panic is caught, or the next access reaches into the surface that was entered.
#[test]
fn an_unwinding_entry_restores_the_world_around_it() {
    let outer = ProbeContext::new();
    let inner = ProbeContext::new();
    outer.with_entered(|| {
        set_probe(1);
        let outcome = quietly(|| {
            inner.with_entered(|| {
                set_probe(2);
                panic!("unwinds through the entry");
            })
        });
        assert!(outcome.is_err());
        assert_eq!(probe(), 1, "back in the outer surface");
    });
    inner.with_entered(|| assert_eq!(probe(), 2));
}

#[test]
fn a_context_dropped_after_its_entry_leaves_the_slot_on_the_world_around_it() {
    std::thread::spawn(|| {
        let before = worlds_dropped();
        let surface = WorldContext::new();
        surface.with_entered(|| with_world(|_| ()));
        drop(surface);
        assert_eq!(worlds_dropped(), before + 1);
        with_world_ref(|_| ());
        assert_eq!(
            worlds_dropped(),
            before + 1,
            "the ambient world is a new one"
        );
    })
    .join()
    .unwrap();
}

#[test]
fn entries_end_in_reverse_order_even_when_the_inner_one_unwinds() {
    let first = ProbeContext::new();
    let second = ProbeContext::new();
    set_probe(5);
    let outcome = quietly(|| {
        first.with_entered(|| {
            set_probe(1);
            second.with_entered(unwind)
        })
    });
    assert!(outcome.is_err());
    assert_eq!(
        probe(),
        5,
        "both entries ended and the ambient world is live"
    );
    drop(second);
    drop(first);
    assert_eq!(probe(), 5);
}

#[test]
fn an_ambient_entry_inside_a_surface_reaches_the_ambient_world() {
    let surface = ProbeContext::new();
    set_probe(3);
    surface.with_entered(|| {
        set_probe(4);
        ProbeContext::with_ambient_entered(|| {
            assert_eq!(probe(), 3);
            set_probe(30);
        });
        assert_eq!(
            probe(),
            4,
            "the surface is live again after the ambient entry"
        );
    });
    assert_eq!(probe(), 30);
}

#[test]
fn an_ambient_entry_unwinds_back_to_the_surface_around_it() {
    let surface = ProbeContext::new();
    surface.with_entered(|| {
        set_probe(8);
        let outcome = quietly(|| ProbeContext::with_ambient_entered(unwind));
        assert!(outcome.is_err());
        assert_eq!(probe(), 8);
    });
}
