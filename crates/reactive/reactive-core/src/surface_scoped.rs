//! [`SurfaceScoped`] and [`surface_scoped!`](crate::surface_scoped): a value the whole thread shares, which a surface can shadow with one of its own.
//!
//! The writing direction, the locale and the control size all have this shape: one thread-wide signal every surface reads, and a per-surface override that takes its place on the surface that sets one, so a canvas can show a variant beside the chrome around it. The macro declares the two signals; the type answers every read and write of them, so the resolution and the skipped no-op writes are written once.

use std::borrow::Borrow;

use crate::RwSignal;

/// A [`surface_scoped!`](crate::surface_scoped) value as the active surface sees it: the thread's signal and the active surface's override.
///
/// `T` is what a surface overrides with. `Value` is what the thread holds and what a read resolves to, which `T` converts into: the same type, unless the thread's value can be absent, as a locale is before one is set.
///
/// Every write that would store the value already there is skipped, so readers re-run only for a change.
pub struct SurfaceScoped<T: 'static, Value: 'static = T> {
    thread: RwSignal<Value>,
    surface: RwSignal<Option<T>>,
}

impl<T: 'static, Value: 'static> Clone for SurfaceScoped<T, Value> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static, Value: 'static> Copy for SurfaceScoped<T, Value> {}

impl<T: 'static, Value: 'static> SurfaceScoped<T, Value> {
    #[doc(hidden)]
    pub fn from_signals(thread: RwSignal<Value>, surface: RwSignal<Option<T>>) -> Self {
        Self { thread, surface }
    }
}

impl<T, Value> SurfaceScoped<T, Value>
where
    T: Clone + Into<Value> + 'static,
    Value: Clone + PartialEq + 'static,
{
    /// Sets the value of every surface without an override of its own.
    pub fn set(self, value: Value) {
        if self.thread.peek_with(|current| *current != value) {
            self.thread.set(value);
        }
    }

    /// Gives the active surface a value of its own, or with `None` hands it back to the thread's. Takes it borrowed so an unchanged value costs no copy.
    pub fn set_surface<Q>(self, value: Option<&Q>)
    where
        Q: PartialEq + ToOwned<Owned = T> + ?Sized,
        T: Borrow<Q>,
    {
        let changed = self
            .surface
            .peek_with(|current| current.as_ref().map(T::borrow) != value);
        if changed {
            self.surface.set(value.map(ToOwned::to_owned));
        }
    }

    /// Reactive read of the active surface's own value, `None` where it follows the thread's.
    pub fn surface(self) -> Option<T> {
        self.surface.get()
    }

    /// Reactive read of the value in force on the active surface. Subscribes to the thread's value only while the surface has none of its own.
    pub fn get(self) -> Value {
        match self.surface.get() {
            Some(value) => value.into(),
            None => self.thread.get(),
        }
    }

    /// Non-reactive read of the value in force on the active surface.
    pub fn peek(self) -> Value {
        match self.surface.peek() {
            Some(value) => value.into(),
            None => self.thread.peek(),
        }
    }
}

/// Declares a [`SurfaceScoped`] value: a thread-wide signal starting at `init`, a per-surface override, the `Context`/`Guard` pair a surface swaps the override with, and a private `fn name() -> SurfaceScoped<..>` the module's public functions go through.
///
/// `as Value` gives the thread a different type from the override, which converts into it; see [`SurfaceScoped`]. The doc comment documents the `Context`.
///
/// ```ignore
/// reactive_core::surface_scoped! {
///     /// The active surface's own locale, in place of the thread's while it has one.
///     scoped locale: String as Option<String> = None;
///     context LocaleContext, LocaleGuard;
/// }
///
/// pub fn current_locale() -> Option<String> {
///     locale().peek()
/// }
/// ```
#[macro_export]
macro_rules! surface_scoped {
    (
        $(#[$ctx_meta:meta])*
        scoped $name:ident : $ty:ty as $value:ty = $init:expr;
        context $ctx:ident, $guard:ident;
    ) => {
        #[allow(unused_imports)]
        pub use self::$name::{$ctx, $guard};

        fn $name() -> $crate::SurfaceScoped<$ty, $value> {
            self::$name::handle()
        }

        // The statics need names of their own, and a module is the one place to give them some without asking the caller for more.
        #[allow(clippy::module_inception)]
        mod $name {
            #[allow(unused_imports)]
            use super::*;

            ::std::thread_local! {
                static THREAD: $crate::RwSignal<$value> =
                    $crate::detached(|| $crate::signal($init));
            }

            $crate::surface_local! {
                $(#[$ctx_meta])*
                slot OVERRIDE: $crate::RwSignal<::std::option::Option<$ty>> =
                    $crate::signal(::std::option::Option::None);
                access with_override, with_override_ref;
                context $ctx, $guard;
            }

            pub(super) fn handle() -> $crate::SurfaceScoped<$ty, $value> {
                $crate::SurfaceScoped::from_signals(
                    THREAD.with(|thread| *thread),
                    with_override_ref(|surface| *surface),
                )
            }
        }
    };
    (
        $(#[$ctx_meta:meta])*
        scoped $name:ident : $ty:ty = $init:expr;
        context $ctx:ident, $guard:ident;
    ) => {
        $crate::surface_scoped! {
            $(#[$ctx_meta])*
            scoped $name: $ty as $ty = $init;
            context $ctx, $guard;
        }
    };
}

#[cfg(test)]
#[path = "surface_scoped_test.rs"]
mod tests;
