use super::*;

/// What a reentrant borrow used to say was `already borrowed: BorrowMutError`, over a backtrace of the runtime's own frames: the call that collided is in there somewhere, the call it collided *with* never is — and that second one is the whole of the diagnosis, because it is the operation still on the stack that came back round.
#[test]
fn a_reentrant_runtime_borrow_names_both_call_sites() {
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        RUNTIME.with(|rt| {
            let _held = rt.borrow_mut();
            let _collides = rt.borrow_mut();
        });
    }));
    std::panic::set_hook(quiet);

    let payload = outcome.expect_err("the second borrow cannot succeed");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        message.contains("`RUNTIME` is already borrowed"),
        "{message}"
    );
    assert_eq!(
        message.matches(file!()).count(),
        2,
        "both sites are named, and both are in this file:\n{message}"
    );
}

struct DropFlag(std::rc::Rc<Cell<bool>>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

/// What a hot-reloaded build's host does before it unloads it: every signal, effect and owner the thread made goes, with what each held, and the thread can carry on in a fresh runtime.
#[test]
fn releasing_a_thread_frees_its_runtime_and_a_later_use_starts_a_fresh_one() {
    std::thread::spawn(|| {
        let dropped = std::rc::Rc::new(Cell::new(false));
        let flag = DropFlag(dropped.clone());
        let source = crate::signal(0);
        let scope = crate::root_scope();
        crate::effect(move || {
            let _held = &flag;
            source.get();
        });
        drop(scope);
        assert!(live_signal_count() > 0 && live_effect_count() > 0 && live_owner_count() > 0);

        crate::thread_exit::release_thread();
        assert!(dropped.get(), "what the effect held is dropped");
        assert_eq!(
            (live_signal_count(), live_effect_count(), live_owner_count()),
            (0, 0, 0)
        );

        let fresh = crate::signal(1);
        assert_eq!(fresh.get(), 1);
        crate::thread_exit::release_thread();
        assert_eq!(
            live_signal_count(),
            0,
            "the fresh runtime is released in turn"
        );
    })
    .join()
    .unwrap();
}

#[test]
fn an_effect_may_dispose_its_own_owner_and_keep_using_what_it_captured() {
    reset_runtime();
    let trigger = crate::signal(0);
    let scope = crate::owner_scope();
    let id = scope.id();
    let captured: Vec<i32> = (1..=3).collect();
    let sum = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = std::rc::Rc::clone(&sum);
    crate::effect(move || {
        if trigger.get() == 1 {
            crate::dispose_owner(id);
            seen.set(captured.iter().sum());
        }
    });
    drop(scope);
    trigger.set(1);
    assert_eq!(sum.get(), 6);
    reset_runtime();
}

#[test]
fn resetting_the_runtime_inside_an_effect_is_refused() {
    reset_runtime();
    let trigger = crate::signal(0);
    let refused = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = std::rc::Rc::clone(&refused);
    crate::effect(move || {
        if trigger.get() == 1 {
            let quiet = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let outcome = std::panic::catch_unwind(reset_runtime);
            std::panic::set_hook(quiet);
            seen.set(outcome.is_err());
        }
    });
    trigger.set(1);
    assert!(refused.get());
    reset_runtime();
}
