//! Autoref specialisation over [`PreviewArg`], so generated code can hand any value to a preview's args without knowing its type, and over [`IntoAction`], so it can log a prop's calls whatever the prop is.
//!
//! A type with [`PreviewArg`] is read through the [`PreviewCtx`] and gets its control. A type without one passes through unchanged and is listed read-only, shown by its `Debug` text when it has one. The choice is made by method resolution on `&&&Probe<T>`, where each tier implements its trait one reference shallower than the tier above, so the first that applies wins.
//!
//! Reach it through the macros, which import the tiers and keep a type error on the line that wrote the value:
//!
//! ```ignore
//! // A value: overridden by its control when its type has one, otherwise passed through.
//! let label = ::telar::__preview_arg!(__preview, "label", "Save");
//! let size = ::telar::__preview_arg!(__preview, "size", type f32, 16.0);
//! // A two-way signal: `RwSignal<T>`, shared with its control when `T` has one.
//! let agree = ::telar::__preview_signal!(__preview, "agree", false);
//! // A type's control, with no value: `ControlKind::ReadOnly` when it has none.
//! let control: ::telar::preview::ControlKind = ::telar::__preview_control!(ButtonSize);
//! // A prop: each call logged under its name when it is a callback, by its arguments' `Debug` text or else their type's name; any other value unchanged.
//! self.on_press = ::telar::__preview_action!(log, "on_press", self.on_press);
//! ```
//!
//! The first argument is a `PreviewCtx` or a reference to one, or for `__preview_action!` the `ActionLog`.

use std::borrow::Borrow;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::OnceLock;

use reactive_core::{RwSignal, signal};

use super::{ActionLog, ControlKind, IntoAction, PreviewArg, PreviewCtx};

/// Carries the type under test. Built from a value with [`Probe::of`], or from a type alone with [`Probe::new`].
pub struct Probe<T>(PhantomData<fn() -> T>);

impl<T> Probe<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }

    pub const fn of(_: &T) -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for Probe<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The tier for a type with [`PreviewArg`].
pub trait ViaPreviewArg {
    type Value;
    fn control(&self) -> ControlKind;
    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: Self::Value) -> Self::Value;
    fn signal(
        &self,
        ctx: &PreviewCtx,
        name: &'static str,
        value: Self::Value,
    ) -> RwSignal<Self::Value>;
}

impl<T: PreviewArg> ViaPreviewArg for &&Probe<T> {
    type Value = T;

    fn control(&self) -> ControlKind {
        T::CONTROL
    }

    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> T {
        ctx.arg(name, value)
    }

    fn signal(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> RwSignal<T> {
        ctx.signal(name, value)
    }
}

/// The tier for a type without [`PreviewArg`] that has `Debug`: read-only, shown by its `Debug` text.
pub trait ViaDebug {
    type Value: 'static;
    fn control(&self) -> ControlKind;
    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: Self::Value) -> Self::Value;
    fn signal(
        &self,
        ctx: &PreviewCtx,
        name: &'static str,
        value: Self::Value,
    ) -> RwSignal<Self::Value>;
}

impl<T: Debug + 'static> ViaDebug for &Probe<T> {
    type Value = T;

    fn control(&self) -> ControlKind {
        ControlKind::ReadOnly
    }

    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> T {
        ctx.args().read_only(name, Some(format!("{value:?}")));
        value
    }

    fn signal(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> RwSignal<T> {
        ctx.args().read_only(name, Some(format!("{value:?}")));
        signal(value)
    }
}

/// The last tier: read-only, with nothing to show.
pub trait ViaOpaque {
    type Value: 'static;
    fn control(&self) -> ControlKind;
    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: Self::Value) -> Self::Value;
    fn signal(
        &self,
        ctx: &PreviewCtx,
        name: &'static str,
        value: Self::Value,
    ) -> RwSignal<Self::Value>;
}

impl<T: 'static> ViaOpaque for Probe<T> {
    type Value = T;

    fn control(&self) -> ControlKind {
        ControlKind::ReadOnly
    }

    fn arg(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> T {
        ctx.args().read_only(name, None);
        value
    }

    fn signal(&self, ctx: &PreviewCtx, name: &'static str, value: T) -> RwSignal<T> {
        ctx.args().read_only(name, None);
        signal(value)
    }
}

macro_rules! callback_traits {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            pub trait $name: Sized {
                fn action(log: ActionLog, name: &'static str) -> Self;
                fn logged(self, log: ActionLog, name: &'static str) -> Self;
            }
        )*
    };
}

callback_traits! {
    /// [`IntoAction`] for a callback with an argument that has no `Debug`, each argument recorded by its type's name.
    IntoOpaqueAction;
    /// [`IntoAction`] for a callback of one borrowed argument, `Rc<dyn Fn(&T)>`, recorded by its `Debug` text. A trait of its own because an impl of `IntoAction` for it would overlap the one for `Rc<dyn Fn(T)>` as coherence will come to see it (rust-lang/rust#56105).
    IntoBorrowedAction;
    /// [`IntoBorrowedAction`] for an argument that has no `Debug`, recorded by its type's name.
    IntoOpaqueBorrowedAction;
}

macro_rules! action_tiers {
    ($($(#[$doc:meta])* $tier:ident: $callback:ident for $probe:ty;)*) => {
        $(
            $(#[$doc])*
            pub trait $tier {
                type Value;
                fn log_calls(&self, value: Self::Value, log: ActionLog, name: &'static str) -> Self::Value;
            }

            impl<T: $callback> $tier for $probe {
                type Value = T;

                fn log_calls(&self, value: T, log: ActionLog, name: &'static str) -> T {
                    value.logged(log, name)
                }
            }
        )*
    };
}

// A callback has at most one of `IntoAction` and `IntoBorrowedAction`, and of their opaque pair, so two tiers can share a depth.
action_tiers! {
    /// The tier for a callback with [`IntoAction`].
    ViaAction: IntoAction for &&Probe<T>;
    /// The tier for a callback with [`IntoBorrowedAction`].
    ViaBorrowedAction: IntoBorrowedAction for &&Probe<T>;
    /// The tier for a callback whose arguments have no `Debug`.
    ViaOpaqueAction: IntoOpaqueAction for &Probe<T>;
    /// The tier for a callback whose borrowed argument has no `Debug`.
    ViaOpaqueBorrowedAction: IntoOpaqueBorrowedAction for &Probe<T>;
}

/// The last tier: a value that is no callback, left as it is.
pub trait ViaInert {
    type Value;
    fn log_calls(&self, value: Self::Value, log: ActionLog, name: &'static str) -> Self::Value;
}

impl<T> ViaInert for Probe<T> {
    type Value = T;

    fn log_calls(&self, value: T, _: ActionLog, _: &'static str) -> T {
        value
    }
}

/// Applies a `#[props(control = …)]` refinement to a probed control, reaching through an `Option` to the control it wraps.
///
/// [`ControlKind::Optional`] holds its inner control by `'static` reference, so the refined inner one needs a home that outlives the call: `inner` is the generated field's own cell, filled once.
pub fn refine(
    probed: ControlKind,
    inner: &'static OnceLock<ControlKind>,
    refinement: fn(ControlKind) -> ControlKind,
) -> ControlKind {
    match probed {
        ControlKind::Optional(control) => {
            ControlKind::Optional(inner.get_or_init(|| refinement(*control)))
        }
        control => refinement(control),
    }
}

/// The [`PreviewCtx`] the macros were handed, by value or by reference.
pub fn ctx_of<C: Borrow<PreviewCtx>>(ctx: &C) -> &PreviewCtx {
    ctx.borrow()
}

/// Reads a value through the preview's args when its type has [`PreviewArg`], and passes it through read-only when it has not. See [`crate::preview::__probe`].
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_arg {
    ($ctx:expr, $name:expr, type $ty:ty, $value:expr $(,)?) => {
        $crate::__preview_probe!(arg, $ctx, $name, $ty, $value)
    };
    ($ctx:expr, $name:expr, $value:expr $(,)?) => {
        $crate::__preview_probe!(arg, $ctx, $name, _, $value)
    };
}

/// A two-way signal for an explicit arg: shared with its control when its type has [`PreviewArg`], a plain signal listed read-only when it has not.
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_signal {
    ($ctx:expr, $name:expr, type $ty:ty, $value:expr $(,)?) => {
        $crate::__preview_probe!(signal, $ctx, $name, $ty, $value)
    };
    ($ctx:expr, $name:expr, $value:expr $(,)?) => {
        $crate::__preview_probe!(signal, $ctx, $name, _, $value)
    };
}

/// What [`__preview_arg!`] and [`__preview_signal!`] expand to: `$value` bound at `$ty` (`_` to infer it) on a line of its own, then handed to the first tier its type reaches.
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_probe {
    ($method:ident, $ctx:expr, $name:expr, $ty:ty, $value:expr) => {{
        #[allow(unused_imports)]
        use $crate::preview::__probe::{ViaDebug as _, ViaOpaque as _, ViaPreviewArg as _};
        let __telar_arg: $ty = $value;
        (&&&$crate::preview::__probe::Probe::of(&__telar_arg)).$method(
            $crate::preview::__probe::ctx_of(&$ctx),
            $name,
            __telar_arg,
        )
    }};
}

/// A prop's value with each call it receives recorded in an [`ActionLog`] under the prop's name when it is a callback, and as it was when it is not.
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_action {
    ($log:expr, $name:expr, $value:expr $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::preview::__probe::{
            ViaAction as _, ViaBorrowedAction as _, ViaInert as _, ViaOpaqueAction as _,
            ViaOpaqueBorrowedAction as _,
        };
        let __telar_value = $value;
        (&&&$crate::preview::__probe::Probe::of(&__telar_value)).log_calls(
            __telar_value,
            $log,
            $name,
        )
    }};
}

/// The control a type gets: its [`PreviewArg::CONTROL`], or `ControlKind::ReadOnly` when it has none.
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_control {
    ($ty:ty $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::preview::__probe::{ViaDebug as _, ViaOpaque as _, ViaPreviewArg as _};
        (&&&$crate::preview::__probe::Probe::<$ty>::new()).control()
    }};
}

#[cfg(test)]
#[path = "probe_test.rs"]
mod tests;
