//! `surface_local!` — declares a thread-local whose contents can be swapped per surface.
//!
//! A per-surface "world" (the layout tree, the overlay registry, focus, ...) is a thread-local singleton.
//! Under M3 several surfaces share one UI thread, so each such world must be swappable: the runner activates
//! a surface's instance around that surface's build/event/frame, and the reactive flush re-enters the
//! instance that owns each effect. This macro generates that swap once — the live instance sits behind a
//! `Cell<*mut RefCell<T>>` (like the reactive runtime's own cell), a private accessor derefs it, and a
//! `Context`/`Guard` pair allocates and activates per-surface instances.
//!
//! The cell holds a raw pointer and has no `Drop`, so no TLS destructor is registered — dlclosing a
//! hot-reload dylib on thread exit stays safe (same reasoning as the reactive runtime cell). The ambient
//! instance is intentionally leaked; per-surface instances are freed when their `Context` drops.

/// Generates a swappable per-surface thread-local plus its `Context`/`Guard`. See the module docs.
///
/// ```ignore
/// reactive_core::surface_local! {
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
            static $slot: ::std::cell::Cell<*mut ::std::cell::RefCell<$ty>> =
                ::std::cell::Cell::new(::std::boxed::Box::into_raw(::std::boxed::Box::new(
                    ::std::cell::RefCell::new($init),
                )));
        }

        #[allow(dead_code)]
        fn $with<R>(f: impl ::std::ops::FnOnce(&mut $ty) -> R) -> R {
            // SAFETY: the pointer always addresses a live, heap-allocated `RefCell<$ty>` — either the
            // leaked ambient instance or a per-surface `Context`'s box that outlives every guard pointing
            // the slot at it. The borrow is released before the closure returns, so swaps never race a borrow.
            $slot.with(|cell| unsafe { f(&mut *(*cell.get()).borrow_mut()) })
        }

        #[allow(dead_code)]
        fn $with_ref<R>(f: impl ::std::ops::FnOnce(&$ty) -> R) -> R {
            // SAFETY: see `$with`.
            $slot.with(|cell| unsafe { f(&*(*cell.get()).borrow()) })
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
                        ::std::cell::RefCell::new($init),
                    )),
                }
            }

            /// Makes this instance the live one until the returned guard drops, which restores the
            /// previously-active instance. Nest by keeping guards in scope; they restore in reverse order.
            #[must_use = "the surface context is only active while this guard is alive"]
            pub fn enter(&self) -> $guard {
                let prev = $slot.with(|cell| cell.replace(self.ptr));
                $guard { prev }
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

        #[must_use = "the surface context is only active while this guard is alive"]
        pub struct $guard {
            prev: *mut ::std::cell::RefCell<$ty>,
        }

        impl ::std::ops::Drop for $guard {
            fn drop(&mut self) {
                $slot.with(|cell| cell.set(self.prev));
            }
        }
    };
}
