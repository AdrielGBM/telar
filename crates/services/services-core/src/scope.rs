//! Ambient values a widget can read without being handed them, scoped to the owner that provided them.
//!
//! Backed by [the owner tree](reactive_core), a scope lives as long as the node that opened it rather than closing when a call returns, so an `on_press` handler that runs long after the build can still read what its subtree was given.

use std::any::Any;

use crate::registry::ServiceError;

/// Makes `service` readable by everything built under the current owner.
///
/// Returns `Err(AlreadyRegistered)` when this owner has already provided that type — a component saying the same thing twice about its own subtree, which is a mistake rather than an intent. Shadowing from a *nested* scope is fine and always was: the walk finds the nearest one.
pub fn provide<T: Any + 'static>(service: T) -> Result<(), ServiceError> {
    if reactive_core::context_provided_here::<Provided<T>>() {
        return Err(ServiceError::AlreadyRegistered);
    }
    reactive_core::provide_context(Provided(service));
    Ok(())
}

/// A newtype so "this owner provided a `T`" is a different question from "a `T` is visible here", which is what tells an owner repeating itself apart from one shadowing its parent.
struct Provided<T>(T);

/// The nearest `T` at or above the current owner, cloned.
pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    reactive_core::with_context::<Provided<T>, _>(|p| p.0.clone())
}

/// The nearest `T` at or above the current owner, read in place — for a service too large to clone, or one that is not `Clone` at all.
pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    reactive_core::with_context::<Provided<T>, _>(|p| f(&p.0))
}

/// Opens a nested scope for the duration of `f`.
///
/// For code that wants a scope around a call and nothing more. The scope it opens outlives `f`, and is disposed with the owner rather than at the closing brace.
pub struct Scope(());

impl Scope {
    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        let _scope = reactive_core::owner_scope();
        f()
    }
}

/// Sets the current owner's context of type `T`, replacing whatever it had.
///
/// The difference from [`provide`] is repetition: a rebuild says the same things about the same subtree, and this is the spelling that means "again".
pub fn set_context<T: Clone + 'static>(value: T) {
    reactive_core::provide_context(Provided(value));
}

/// The nearest context of type `T`. `None` before anything set one.
pub fn context<T: Clone + 'static>() -> Option<T> {
    try_inject::<T>()
}

#[cfg(test)]
#[path = "scope.test.rs"]
mod tests;
