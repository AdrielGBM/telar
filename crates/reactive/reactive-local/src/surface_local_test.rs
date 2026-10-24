crate::surface_local! {
    slot PROBE: u32 = 0;
    access with_probe, with_probe_ref;
    context ProbeContext, ProbeGuard;
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
    context WorldContext, WorldGuard;
}

fn worlds_dropped() -> u32 {
    WORLDS_DROPPED.load(std::sync::atomic::Ordering::Relaxed)
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

/// A guard holds a pointer it restores, which may be the ambient world: freeing it then would leave the slot pointing at freed memory once the guard drops.
#[test]
fn the_ambient_world_is_kept_while_a_guard_is_entered_and_released_after() {
    std::thread::spawn(|| {
        let before = worlds_dropped();
        let surface = WorldContext::new();
        let entered = surface.enter();
        let _ambient = WorldContext::enter_ambient();
        with_world_ref(|_| ());
        crate::thread_exit::release_thread();
        assert_eq!(worlds_dropped(), before);
        drop(_ambient);
        drop(entered);
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
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_probe(|_| with_probe(|_| ()));
    }));
    std::panic::set_hook(quiet);

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
        let _entered = other.enter();
        with_probe(|inner| {
            assert_eq!(*inner, 0, "the second surface has its own value");
            *inner = 2;
        });
    });
    with_probe_ref(|ambient| assert_eq!(*ambient, 1, "and the first kept its own"));
}

/// A shared borrow blocks a mutable one just the same, and that pairing is the one a reader would least expect to be a collision at all.
#[test]
fn a_read_that_blocks_a_write_is_reported_too() {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_probe_ref(|_| with_probe(|_| ()));
    }));
    std::panic::set_hook(quiet);

    let payload = outcome.expect_err("a write cannot join a live read");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(message.contains("`PROBE` is already borrowed"), "{message}");
}
