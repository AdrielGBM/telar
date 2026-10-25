//! `surface_local!` — declares a thread-local whose contents can be swapped per surface.
//!
//! A per-surface "world" (the layout tree, the overlay registry, focus, ...) is a thread-local singleton. Under M3 several surfaces share one UI thread, so each such world must be swappable: the runner activates a surface's instance around that surface's build/event/frame, and the reactive flush re-enters the instance that owns each effect. This macro generates that swap once — the live instance sits behind a `Cell<*mut RefCell<T>>` (like the reactive runtime's own cell), a private accessor derefs it, and a `Context` allocates per-surface instances and makes one live for the duration of a closure, which borrows the `Context` for as long as its instance is live and restores the previous one on return and on unwind alike (a guard could be forgotten or dropped out of order, leaving the slot on an instance already freed).
//!
//! The cell holds raw pointers, as [`dropless_thread_local!`](crate::dropless_thread_local) requires. The ambient instance is made the first time the slot is reached and freed through [`thread_exit`](crate::thread_exit), so a guest's is gone once its host unloads it; per-surface instances are freed when their `Context` drops.
//!
//! Both instances build their contents inside [`detached`](crate::detached), because a slot initialises on first access and that access is somebody's build. Without it a surface's world is adopted by whichever reactive owner happened to be running — see that module.

/// Generates a swappable per-surface thread-local plus its `Context`. See the module docs.
///
/// ```
/// telar_reactive_local::surface_local! {
///     slot DEPTH: u32 = 0;
///     access with_depth, with_depth_ref;
///     context DepthContext;
/// }
///
/// let surface = DepthContext::new();
/// surface.with_entered(|| with_depth(|depth| *depth = 1));
/// assert_eq!(with_depth_ref(|depth| *depth), 0);
/// assert_eq!(surface.with_entered(|| with_depth_ref(|depth| *depth)), 1);
/// ```
///
/// A context cannot go while its world is live, because the entry borrows it:
///
/// ```compile_fail,E0505
/// telar_reactive_local::surface_local! {
///     slot DEPTH: u32 = 0;
///     access with_depth, with_depth_ref;
///     context DepthContext;
/// }
///
/// let surface = DepthContext::new();
/// surface.with_entered(|| drop(surface));
/// ```
#[macro_export]
macro_rules! surface_local {
    (
        $(#[$ctx_meta:meta])*
        slot $slot:ident : $ty:ty = $init:expr;
        access $with:ident, $with_ref:ident;
        context $ctx:ident;
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
            // SAFETY: the pointer always addresses a live, heap-allocated `RefCell` — the ambient instance, which is only freed while nothing holds or enters it, or a per-surface box whose `Context` is borrowed by the `with_entered` call that made it live. The borrow is released before the closure returns, so swaps never race a borrow.
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

            /// Makes this instance the live one while `f` runs, and restores whichever was live before once `f` returns or unwinds. Entries nest: an inner one restores the outer.
            pub fn with_entered<R>(&self, f: impl ::std::ops::FnOnce() -> R) -> R {
                Self::entered(self.ptr, f)
            }

            /// Makes the *ambient* instance — the one that exists before any surface is built, and the only world a single-surface app ever has — live while `f` runs, as [`with_entered`](Self::with_entered) does a surface's.
            ///
            /// What the reactive flush needs for an effect owned by [`SurfaceHandle::NONE`](crate::SurfaceHandle::NONE): it was registered outside any surface, so its world is this one, and running it against whichever surface happened to be entered when the signal fired would resolve its layout, overlays and focus in somebody else's. Reachable as soon as one app has both — a window tree that never built a surface and a second tree that did.
            pub fn with_ambient_entered<R>(f: impl ::std::ops::FnOnce() -> R) -> R {
                let ambient = $slot.with(|cell| {
                    $crate::SurfaceSlot::ambient(cell, || $crate::detached(|| $init), || $slot.with($crate::SurfaceSlot::release))
                });
                Self::entered(ambient, f)
            }

            fn entered<R>(
                instance: *mut ::std::cell::RefCell<$ty>,
                f: impl ::std::ops::FnOnce() -> R,
            ) -> R {
                struct Restore(*mut ::std::cell::RefCell<$ty>);

                impl ::std::ops::Drop for Restore {
                    fn drop(&mut self) {
                        $slot.with(|cell| $crate::SurfaceSlot::leave(cell, self.0));
                    }
                }

                let _restore = Restore($slot.with(|cell| $crate::SurfaceSlot::enter(cell, instance)));
                f()
            }
        }

        impl ::std::default::Default for $ctx {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::ops::Drop for $ctx {
            fn drop(&mut self) {
                let live = $slot.with(|cell| cell.get().live);
                ::std::assert!(
                    live != self.ptr,
                    ::std::concat!(
                        "a `",
                        ::std::stringify!($ctx),
                        "` was dropped while its world was live"
                    ),
                );
                // SAFETY: the slot points at this box only during a `with_entered` call, which borrows `self` and so cannot be under way, as the assert above checks; nothing else holds the pointer.
                unsafe { drop(::std::boxed::Box::from_raw(self.ptr)) };
            }
        }
    };
}

/// The live instance of a [`surface_local!`] world plus the ambient one it started as.
///
/// Both are kept because "restore what was active before" and "activate the world of an effect that belongs to no surface" are different questions, and only the second one can be answered from a saved pointer that no entry is holding.
///
/// `#[doc(hidden)]` — public only because macro expansion lands in the calling crate, which has to be able to name it. Nothing outside the macro should construct one: the fields are raw pointers the expansion dereferences under a SAFETY note that only holds for the ones it makes itself.
#[doc(hidden)]
pub struct SurfaceSlot<T> {
    /// Null until the slot is first reached, and again once the ambient instance it pointed at is released.
    pub live: *mut std::cell::RefCell<T>,
    pub ambient: *mut std::cell::RefCell<T>,
    /// How many entries are under way, each holding a pointer it restores: while any is, the ambient instance may be one of them and is not released.
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

    /// The live instance: the one an entry made live, or the ambient one, made first if nothing has.
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

    /// Makes `instance` live and hands back what was, for the entry to restore.
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

    /// Frees the ambient instance, leaving the slot to make a fresh one should it be reached again. One an entry or a borrow still holds is kept, and released once it is reached again and its thread released after that.
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
        // SAFETY: no entry or borrow holds it, and the slot no longer points at it, so this was the last pointer to the box `ambient` made.
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
// The macro generates a whole Context API; this probe exercises the borrow paths and not all of it.
#[allow(dead_code)]
#[path = "surface_local_test.rs"]
mod tests;
