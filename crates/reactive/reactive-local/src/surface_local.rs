//! `surface_local!` — declares a thread-local whose contents can be swapped per surface.
//!
//! A per-surface "world" (the layout tree, the overlay registry, focus, ...) is a thread-local singleton. Under M3 several surfaces share one UI thread, so each such world must be swappable: the runner activates a surface's instance around that surface's build/event/frame, and the reactive flush re-enters the instance that owns each effect. This macro generates that swap once — the live instance sits behind a `Cell<*mut RefCell<T>>` (like the reactive runtime's own cell), a private accessor derefs it, and a `Context`/`Guard` pair allocates and activates per-surface instances.
//!
//! The cell holds raw pointers, as [`dropless_thread_local!`](crate::dropless_thread_local) requires. The ambient instance is made the first time the slot is reached and freed through [`thread_exit`](crate::thread_exit), so a guest's is gone once its host unloads it; per-surface instances are freed when their `Context` drops.
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
        $crate::dropless_thread_local! {
            static $slot: ::std::cell::Cell<$crate::SurfaceSlot<$ty>> = const {
                ::std::cell::Cell::new($crate::SurfaceSlot::EMPTY)
            };
        }

        // `#[track_caller]` so the recorded site is the caller that reached this world, not this generated accessor.
        #[allow(dead_code)]
        #[track_caller]
        fn $with<R>(f: impl ::std::ops::FnOnce(&mut $ty) -> R) -> R {
            // SAFETY: the pointer always addresses a live, heap-allocated `RefCell` — the ambient instance, which is only freed while nothing holds or enters it, or a per-surface box that outlives every guard pointing the slot at it. The borrow is released before the closure returns, so swaps never race a borrow.
            $slot.with(|cell| {
                let live = $crate::SurfaceSlot::live(cell, || $crate::detached(|| $init), || $slot.with($crate::SurfaceSlot::release));
                let borrowed = unsafe { (*live).try_borrow_mut() };
                match borrowed {
                    ::std::result::Result::Ok(mut guard) => {
                        cell.set(cell.get().borrowed_at(::std::panic::Location::caller()));
                        f(&mut *guard)
                    }
                    ::std::result::Result::Err(_) => $crate::reentry::borrow_collision(
                        ::std::stringify!($slot),
                        cell.get().last_borrow,
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
                let live = $crate::SurfaceSlot::live(cell, || $crate::detached(|| $init), || $slot.with($crate::SurfaceSlot::release));
                let borrowed = unsafe { (*live).try_borrow() };
                match borrowed {
                    ::std::result::Result::Ok(guard) => {
                        cell.set(cell.get().borrowed_at(::std::panic::Location::caller()));
                        f(&*guard)
                    }
                    ::std::result::Result::Err(_) => $crate::reentry::borrow_collision(
                        ::std::stringify!($slot),
                        cell.get().last_borrow,
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
            /// Allocates a fresh, inactive per-surface instance whose reactive state belongs to nobody, so nothing ever frees it.
            pub fn new() -> Self {
                $crate::detached(Self::new_owned)
            }

            /// Allocates a fresh, inactive per-surface instance whose reactive state belongs to the active owner, for a surface that frees its worlds when it goes: build it inside `reactive_core::in_surface_world`.
            pub fn new_owned() -> Self {
                Self {
                    ptr: ::std::boxed::Box::into_raw(::std::boxed::Box::new(
                        ::std::cell::RefCell::new($init),
                    )),
                }
            }

            /// Makes this instance the live one until the returned guard drops, which restores the previously-active instance. Nest by keeping guards in scope; they restore in reverse order.
            #[must_use = "the surface context is only active while this guard is alive"]
            pub fn enter(&self) -> $guard {
                $guard {
                    prev: $slot.with(|cell| $crate::SurfaceSlot::enter(cell, self.ptr)),
                }
            }

            /// Makes the *ambient* instance — the one that exists before any surface is built, and the only world a single-surface app ever has — live until the returned guard drops.
            ///
            /// What the reactive flush needs for an effect owned by [`SurfaceHandle::NONE`](crate::SurfaceHandle::NONE): it was registered outside any surface, so its world is this one, and running it against whichever surface happened to be entered when the signal fired would resolve its layout, overlays and focus in somebody else's. Reachable as soon as one app has both — a window tree that never built a surface and a second tree that did.
            #[must_use = "the surface context is only active while this guard is alive"]
            pub fn enter_ambient() -> $guard {
                $guard {
                    prev: $slot.with(|cell| {
                        let ambient = $crate::SurfaceSlot::ambient(cell, || $crate::detached(|| $init), || $slot.with($crate::SurfaceSlot::release));
                        $crate::SurfaceSlot::enter(cell, ambient)
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
                $slot.with(|cell| $crate::SurfaceSlot::leave(cell, self.prev));
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
    /// Null until the slot is first reached, and again once the ambient instance it pointed at is released.
    pub live: *mut std::cell::RefCell<T>,
    pub ambient: *mut std::cell::RefCell<T>,
    /// How many guards are alive, each holding a pointer it restores: while any is, the ambient instance may be one of them and is not released.
    pub entered: usize,
    /// Whether the ambient instance outlived the release registered for it, held when it ran, so the next access registers another.
    pub kept: bool,
    /// Where the last borrow of this slot to succeed was taken, so a collision names what it collided with rather than only itself. See [`crate::reentry`].
    pub last_borrow: Option<&'static std::panic::Location<'static>>,
}

impl<T> SurfaceSlot<T> {
    #[doc(hidden)]
    pub const EMPTY: Self = Self {
        live: std::ptr::null_mut(),
        ambient: std::ptr::null_mut(),
        entered: 0,
        kept: false,
        last_borrow: None,
    };

    #[doc(hidden)]
    pub fn borrowed_at(mut self, at: &'static std::panic::Location<'static>) -> Self {
        self.last_borrow = Some(at);
        self
    }

    /// The ambient instance, made by `init` the first time it is asked for, with `release` handed to [`on_release`](crate::thread_exit::on_release) to free it.
    #[doc(hidden)]
    pub fn ambient(
        cell: &std::cell::Cell<Self>,
        init: impl FnOnce() -> T,
        release: fn(),
    ) -> *mut std::cell::RefCell<T> {
        Self::register_kept(cell, release);
        let existing = cell.get().ambient;
        if !existing.is_null() {
            return existing;
        }
        let ambient = Box::into_raw(Box::new(std::cell::RefCell::new(init())));
        let mut slot = cell.get();
        slot.ambient = ambient;
        cell.set(slot);
        crate::thread_exit::on_release(release);
        ambient
    }

    /// The live instance: the one a guard entered, or the ambient one, made first if nothing has.
    #[doc(hidden)]
    pub fn live(
        cell: &std::cell::Cell<Self>,
        init: impl FnOnce() -> T,
        release: fn(),
    ) -> *mut std::cell::RefCell<T> {
        Self::register_kept(cell, release);
        let live = cell.get().live;
        if !live.is_null() {
            return live;
        }
        let ambient = Self::ambient(cell, init, release);
        let mut slot = cell.get();
        slot.live = ambient;
        cell.set(slot);
        ambient
    }

    fn register_kept(cell: &std::cell::Cell<Self>, release: fn()) {
        let mut slot = cell.get();
        if slot.kept {
            slot.kept = false;
            cell.set(slot);
            crate::thread_exit::on_release(release);
        }
    }

    /// Makes `instance` live and hands back what was, for the guard to restore.
    #[doc(hidden)]
    pub fn enter(
        cell: &std::cell::Cell<Self>,
        instance: *mut std::cell::RefCell<T>,
    ) -> *mut std::cell::RefCell<T> {
        let mut slot = cell.get();
        let prev = std::mem::replace(&mut slot.live, instance);
        slot.entered += 1;
        cell.set(slot);
        prev
    }

    #[doc(hidden)]
    pub fn leave(cell: &std::cell::Cell<Self>, prev: *mut std::cell::RefCell<T>) {
        let mut slot = cell.get();
        slot.live = prev;
        slot.entered -= 1;
        cell.set(slot);
    }

    /// Frees the ambient instance, leaving the slot to make a fresh one should it be reached again. One a guard or a borrow still holds is kept, and released once it is reached again and its thread released after that.
    #[doc(hidden)]
    pub fn release(cell: &std::cell::Cell<Self>) {
        let mut slot = cell.get();
        let ambient = slot.ambient;
        if ambient.is_null() {
            return;
        }
        if slot.entered > 0 || unsafe { (*ambient).try_borrow_mut() }.is_err() {
            slot.kept = true;
            cell.set(slot);
            return;
        }
        slot.live = std::ptr::null_mut();
        slot.ambient = std::ptr::null_mut();
        slot.last_borrow = None;
        cell.set(slot);
        // SAFETY: no guard or borrow holds it, and the slot no longer points at it, so this was the last pointer to the box `ambient` made.
        drop(unsafe { Box::from_raw(ambient) });
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
#[path = "surface_local_test.rs"]
mod tests;
