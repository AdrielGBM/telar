//! `surface_local!` — declares a thread-local whose contents can be swapped per surface.
//!
//! A per-surface "world" (the layout tree, the overlay registry, focus, ...) is a thread-local singleton. Under M3 several surfaces share one UI thread, so each such world must be swappable: the runner activates a surface's instance around that surface's build/event/frame, and the reactive flush re-enters the instance that owns each effect. This macro generates that swap once — the live instance sits behind a `Cell<*mut RefCell<T>>` (like the reactive runtime's own cell), a private accessor derefs it, and a `Context`/`Guard` pair allocates and activates per-surface instances.
//!
//! The cell holds a raw pointer and has no `Drop`, so no TLS destructor is registered — dlclosing a hot-reload dylib on thread exit stays safe (same reasoning as the reactive runtime cell). The ambient instance is intentionally leaked; per-surface instances are freed when their `Context` drops.
//!
//! Both instances build their contents inside [`detached`](crate::detached), because a slot initialises on first access and that access is somebody's build. Without it a surface's world is adopted by whichever reactive owner happened to be running — see that module.

/// Generates a swappable per-surface thread-local plus its `Context`/`Guard`. See the module docs.
///
/// ```ignore
/// telar_reactive_core::surface_local! {
///     slot LAYOUT: LayoutRuntime = LayoutRuntime::new();
///     access with_layout, with_layout_ref;
///     context LayoutContext, LayoutGuard;
/// }
/// ```
#[macro_export]
macro_rules! surface_local {
    (
        $(#[$ctx_meta:meta])*
        slot $slot:ident : $ty:ty = $init:expr;
        access $with:ident, $with_ref:ident;
        context $ctx:ident, $guard:ident;
    ) => {
        thread_local! {
            static $slot: ::std::cell::Cell<$crate::SurfaceSlot<$ty>> = {
                let ambient = ::std::boxed::Box::into_raw(::std::boxed::Box::new(
                    ::std::cell::RefCell::new($crate::detached(|| $init)),
                ));
                ::std::cell::Cell::new($crate::SurfaceSlot {
                    live: ambient,
                    ambient,
                    last_borrow: ::std::option::Option::None,
                })
            };
        }

        // `#[track_caller]` so the recorded site is the caller that reached this world, not this generated accessor.
        #[allow(dead_code)]
        #[track_caller]
        fn $with<R>(f: impl ::std::ops::FnOnce(&mut $ty) -> R) -> R {
            // SAFETY: the pointer always addresses a live, heap-allocated `RefCell` — the leaked ambient instance or a per-surface box that outlives every guard pointing the slot at it. The borrow is released before the closure returns, so swaps never race a borrow.
            $slot.with(|cell| {
                let slot = cell.get();
                let borrowed = unsafe { (*slot.live).try_borrow_mut() };
                match borrowed {
                    ::std::result::Result::Ok(mut guard) => {
                        cell.set(slot.borrowed_at(::std::panic::Location::caller()));
                        f(&mut *guard)
                    }
                    ::std::result::Result::Err(_) => $crate::reentry::borrow_collision(
                        ::std::stringify!($slot),
                        slot.last_borrow,
                        ::std::panic::Location::caller(),
                    ),
                }
            })
        }

        #[allow(dead_code)]
        #[track_caller]
        fn $with_ref<R>(f: impl ::std::ops::FnOnce(&$ty) -> R) -> R {
            // SAFETY: see `$with`.
            $slot.with(|cell| {
                let slot = cell.get();
                let borrowed = unsafe { (*slot.live).try_borrow() };
                match borrowed {
                    ::std::result::Result::Ok(guard) => {
                        cell.set(slot.borrowed_at(::std::panic::Location::caller()));
                        f(&*guard)
                    }
                    ::std::result::Result::Err(_) => $crate::reentry::borrow_collision(
                        ::std::stringify!($slot),
                        slot.last_borrow,
                        ::std::panic::Location::caller(),
                    ),
                }
            })
        }

        $(#[$ctx_meta])*
        pub struct $ctx {
            ptr: *mut ::std::cell::RefCell<$ty>,
        }

        impl $ctx {
            /// Allocates a fresh, inactive per-surface instance.
            pub fn new() -> Self {
                Self {
                    ptr: ::std::boxed::Box::into_raw(::std::boxed::Box::new(
                        ::std::cell::RefCell::new($crate::detached(|| $init)),
                    )),
                }
            }

            /// Makes this instance the live one until the returned guard drops, which restores the previously-active instance. Nest by keeping guards in scope; they restore in reverse order.
            #[must_use = "the surface context is only active while this guard is alive"]
            pub fn enter(&self) -> $guard {
                $guard {
                    prev: $slot.with(|cell| {
                        let mut slot = cell.get();
                        let prev = ::std::mem::replace(&mut slot.live, self.ptr);
                        cell.set(slot);
                        prev
                    }),
                }
            }

            /// Makes the *ambient* instance — the one that exists before any surface is built, and the only world a single-surface app ever has — live until the returned guard drops.
            ///
            /// What the reactive flush needs for an effect owned by [`SurfaceHandle::NONE`](crate::SurfaceHandle::NONE): it was registered outside any surface, so its world is this one, and running it against whichever surface happened to be entered when the signal fired would resolve its layout, overlays and focus in somebody else's. Reachable as soon as one app has both — a window tree that never built a surface and a second tree that did.
            #[must_use = "the surface context is only active while this guard is alive"]
            pub fn enter_ambient() -> $guard {
                $guard {
                    prev: $slot.with(|cell| {
                        let mut slot = cell.get();
                        let prev = ::std::mem::replace(&mut slot.live, slot.ambient);
                        cell.set(slot);
                        prev
                    }),
                }
            }
        }

        impl ::std::default::Default for $ctx {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::ops::Drop for $ctx {
            fn drop(&mut self) {
                // A matching guard has restored the previous instance, so this box is not the live pointer.
                unsafe { drop(::std::boxed::Box::from_raw(self.ptr)) };
            }
        }

        /// Keeps its surface's instance live. Dropping it restores whichever instance was active before.
        #[must_use = "the surface context is only active while this guard is alive"]
        pub struct $guard {
            prev: *mut ::std::cell::RefCell<$ty>,
        }

        impl ::std::ops::Drop for $guard {
            fn drop(&mut self) {
                $slot.with(|cell| {
                    let mut slot = cell.get();
                    slot.live = self.prev;
                    cell.set(slot);
                });
            }
        }
    };
}

/// The live instance of a [`surface_local!`] world plus the ambient one it started as.
///
/// Both are kept because "restore what was active before" and "activate the world of an effect that belongs to no surface" are different questions, and only the second one can be answered from a saved pointer that no guard is holding.
///
/// `#[doc(hidden)]` — public only because macro expansion lands in the calling crate, which has to be able to name it. Nothing outside the macro should construct one: the fields are raw pointers the expansion dereferences under a SAFETY note that only holds for the ones it makes itself.
#[doc(hidden)]
pub struct SurfaceSlot<T> {
    pub live: *mut std::cell::RefCell<T>,
    pub ambient: *mut std::cell::RefCell<T>,
    /// Where the last borrow of this slot to succeed was taken, so a collision names what it collided with rather than only itself. See [`crate::reentry`].
    pub last_borrow: Option<&'static std::panic::Location<'static>>,
}

impl<T> SurfaceSlot<T> {
    #[doc(hidden)]
    pub fn borrowed_at(mut self, at: &'static std::panic::Location<'static>) -> Self {
        self.last_borrow = Some(at);
        self
    }
}

// Hand-written so the derives do not require `T: Copy`; a pair of raw pointers is `Copy` whatever they point at, which is what the `Cell` holding them needs.
impl<T> Clone for SurfaceSlot<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SurfaceSlot<T> {}

#[cfg(test)]
// The macro generates a whole Context/Guard API; this probe exercises the borrow paths and not all of it.
#[allow(dead_code)]
mod tests {
    crate::surface_local! {
        slot PROBE: u32 = 0;
        access with_probe, with_probe_ref;
        context ProbeContext, ProbeGuard;
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
}
