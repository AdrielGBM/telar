//! State whose handle outlives the build that first reached for it — kept in a thread-local or a store keyed by slot — must be created here, or that build's disposal frees it and every later reader holds a dead handle; state that lives as long as one surface wants `reactive_core::in_surface_world` instead.

use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Runs `f` with anything it creates belonging to no reactive owner.
pub fn detached<R>(f: impl FnOnce() -> R) -> R {
    // Balanced across an unwind, to the standard the owner stack and `batch_depth` both meet: leaving this raised would detach every creation for the rest of the thread's life, and a leak surfaces nowhere near what caused it.
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _restore = Restore;
    f()
}

/// Whether the caller is inside [`detached`].
pub fn is_detached() -> bool {
    DEPTH.with(Cell::get) > 0
}

#[cfg(test)]
#[path = "detached_test.rs"]
mod tests;
