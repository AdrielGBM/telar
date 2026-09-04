//! Building state that belongs to a world rather than to whoever first reached for it.
//!
//! A [`surface_local!`](crate::surface_local) slot initialises lazily, on the first access — and that access is whatever code happened to touch the world first, which in a UI is some component deep inside a build. The reactive runtime attributes what a build creates to the owner that build is running under, so a surface-lifetime signal created inside the first row of a list becomes that row's, and disposing the row frees a signal the whole surface reads.
//!
//! [`detached`] is the seam. It lives here rather than in `reactive-core` because the macro that needs it does, and `reactive-local` cannot depend on the crate that depends on it. `reactive-core` reads [`is_detached`] when deciding what to attribute, and re-exports `detached` for the plain thread-locals that have the same problem without going through the macro.
//!
//! # Why nothing catches this for you
//!
//! Every reactive handle is `Copy` — a signal, a memo, an `Animated` — because each is an id into the runtime's arena rather than the state itself. That is what lets a closure read one without an `Rc` bump, and it is the same trade Leptos and Dioxus each landed on. What it costs is the compile error: nothing stops a handle from being copied out of the scope that owns it and into something that outlives it, so a lifetime mistake that used to be unwritable is now a runtime one instead.
//!
//! The rule that replaces the compiler here is short. **State whose handle is kept somewhere the tree does not own — a thread-local, a store keyed by slot, anything the reader can reach after the widget is gone — has to be created in here.** State a view builds and reads within its own scope wants the ordinary constructors, and is freed with the view.
//!
//! Detached state is freed by nothing: whoever keeps it is now its owner, and dropping it from wherever it is kept is the disposal. That is the point and the cost both — a handle that wants to check rather than assume has `RwSignal::is_alive` and the `try_*` reads over in `reactive-core`.

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
