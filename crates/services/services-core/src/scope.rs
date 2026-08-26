//! Ambient values a widget can read without being handed them, scoped to the owner that provided them.
//!
//! This used to be a per-surface stack of registries, pushed and popped around a call. That shape had one
//! thing wrong with it and it was not the storage: **a call-stack scope closes when the call returns**, and
//! an `on_press` handler runs long after the build that made it did. So the values a component wanted its
//! subtree to read were readable only *during* the build, which is the one moment nothing needs to ask.
//!
//! Backed by [the owner tree](reactive_core) instead, a scope lives as long as the node that opened it.

use std::any::Any;

use crate::registry::ServiceError;

/// Makes `service` readable by everything built under the current owner.
///
/// Returns `Err(AlreadyRegistered)` when this owner has already provided that type — a component saying the
/// same thing twice about its own subtree, which is a mistake rather than an intent. Shadowing from a
/// *nested* scope is fine and always was: the walk finds the nearest one.
pub fn provide<T: Any + 'static>(service: T) -> Result<(), ServiceError> {
    if reactive_core::context_provided_here::<Provided<T>>() {
        return Err(ServiceError::AlreadyRegistered);
    }
    reactive_core::provide_context(Provided(service));
    Ok(())
}

/// A newtype so "this owner provided a `T`" is a different question from "a `T` is visible here", which is
/// what tells an owner repeating itself apart from one shadowing its parent.
struct Provided<T>(T);

/// The nearest `T` at or above the current owner, cloned.
pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    reactive_core::with_context::<Provided<T>, _>(|p| p.0.clone())
}

/// The nearest `T` at or above the current owner, read in place — for a service too large to clone, or one
/// that is not `Clone` at all.
pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    reactive_core::with_context::<Provided<T>, _>(|p| f(&p.0))
}

/// Opens a nested scope for the duration of `f`.
///
/// Kept as a spelling for code that wants a scope around a call and nothing more. Note what it no longer
/// does: the scope it opens outlives `f`, and is disposed with the owner rather than at the closing brace.
pub struct Scope(());

impl Scope {
    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        let _scope = reactive_core::owner_scope();
        f()
    }
}

/// Sets the current owner's context of type `T`, replacing whatever it had.
///
/// The difference from [`provide`] is repetition: a rebuild says the same things about the same subtree, and
/// this is the spelling that means "again". It used to be implemented by providing an `Rc<RefCell<T>>` once
/// and writing through the cell forever after, because the only scope available outlived every build of its
/// content and would refuse the second `provide`. An owner per build removes the reason for the cell.
pub fn set_context<T: Clone + 'static>(value: T) {
    reactive_core::provide_context(Provided(value));
}

/// The nearest context of type `T`. `None` before anything set one.
pub fn context<T: Clone + 'static>() -> Option<T> {
    try_inject::<T>()
}

#[cfg(test)]
mod context_tests {
    use super::*;

    /// A context is written by every build, not provided by the first one — or a rebuilt panel would still be
    /// showing the page, the radius and the config the build before it was given.
    #[test]
    fn a_second_build_replaces_the_context_the_first_one_set() {
        #[derive(Clone, PartialEq, Debug)]
        struct Ctx(&'static str);

        Scope::with(|| {
            set_context(Ctx("first"));
            set_context(Ctx("second"));
            assert_eq!(context::<Ctx>(), Some(Ctx("second")));
        });
    }

    /// Nothing set is `None` rather than a default, so a widget built outside a surface says so.
    #[test]
    fn an_unset_context_is_absent() {
        #[derive(Clone, PartialEq, Debug)]
        struct Unset(u8);

        Scope::with(|| assert_eq!(context::<Unset>(), None));
    }

    /// The reason this phase exists. A handler runs long after the build that made it returned, and a
    /// call-stack scope has closed by then — so the value a component provided for its own subtree was
    /// readable everywhere except from the events that subtree raises.
    ///
    /// **Backing the registry with the tree is only half of it.** A handler is a plain closure: when it runs
    /// there is no owner stack, so an ambient read resolves against the surface root rather than against the
    /// component. Something has to put it back in its owner. Here that is explicit; in a real tree it is
    /// `dispatch_container_event`, which re-enters the child's owner around `on_event` the same way the
    /// reactive flush re-enters an effect's surface.
    #[test]
    fn a_context_provided_during_a_build_is_readable_from_a_handler_that_fires_later() {
        #[derive(Clone, PartialEq, Debug)]
        struct Desk(u8);

        let scope = reactive_core::owner_scope();
        let owner = scope.id();
        set_context(Desk(7));
        let handler: Box<dyn Fn() -> Option<Desk>> = Box::new(context::<Desk>);
        drop(scope);

        assert_eq!(
            reactive_core::with_owner(Some(owner), &handler),
            Some(Desk(7)),
            "the build is long over"
        );

        reactive_core::dispose_owner(owner);
        assert_eq!(
            reactive_core::with_owner(Some(owner), &handler),
            None,
            "and disposing the owner takes it away"
        );
    }
}
