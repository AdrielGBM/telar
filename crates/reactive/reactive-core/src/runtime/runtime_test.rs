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
