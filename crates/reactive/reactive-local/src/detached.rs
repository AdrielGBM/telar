//! Building state that belongs to a world rather than to whoever first reached for it.
//!
//! A [`surface_local!`](crate::surface_local) slot initialises lazily, on the first access — and that access
//! is whatever code happened to touch the world first, which in a UI is some component deep inside a build.
//! The reactive runtime attributes what a build creates to the owner that build is running under, so a
//! surface-lifetime signal created inside the first row of a list becomes that row's, and disposing the row
//! frees a signal the whole surface reads.
//!
//! [`detached`] is the seam. It lives here rather than in `reactive-core` because the macro that needs it
//! does, and `reactive-local` cannot depend on the crate that depends on it. `reactive-core` reads
//! [`is_detached`] when deciding what to attribute, and re-exports `detached` for the plain thread-locals
//! that have the same problem without going through the macro.

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
mod tests {
    use super::*;

    #[test]
    fn a_panic_inside_leaves_the_depth_where_it_found_it() {
        let outcome = std::panic::catch_unwind(|| {
            detached(|| {
                detached(|| panic!("boom"));
            })
        });
        assert!(outcome.is_err());
        assert!(
            !is_detached(),
            "an unwind must not leave the thread detached"
        );
    }
}
