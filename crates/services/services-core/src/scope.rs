use std::any::Any;

use crate::registry::ServiceRegistry;

reactive_local::surface_local! {
    /// A per-surface service stack. Under M3 many surfaces share one UI thread, so each surface owns its own
    /// stack; the runner activates it with [`ServiceContext::enter`] around the surface's build/event/frame
    /// (and the reactive flush re-enters it for that surface's effects), so `provide`/`try_inject` resolve
    /// against the surface whose context is active — the generic per-surface context primitive.
    slot STACK: Vec<ServiceRegistry> = vec![ServiceRegistry::new()];
    access with_stack, with_stack_ref;
    context ServiceContext, ServiceGuard;
}

pub fn provide<T: Any + 'static>(service: T) -> Result<(), crate::registry::ServiceError> {
    with_stack(|stack| {
        stack
            .last_mut()
            .expect("service stack is empty — this is a bug")
            .insert(service)
    })
}

pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    with_stack_ref(|stack| stack.last().and_then(|scope| scope.get::<T>()).cloned())
}

pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    with_stack_ref(|stack| stack.last().and_then(|scope| scope.get::<T>()).map(f))
}

pub struct Scope(());

impl Scope {
    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        with_stack(|stack| {
            let mut new_scope = ServiceRegistry::new();
            if let Some(parent) = stack.last() {
                new_scope.merge_from(parent);
            }
            stack.push(new_scope);
        });
        struct PopGuard;
        impl Drop for PopGuard {
            fn drop(&mut self) {
                with_stack(|stack| {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                });
            }
        }
        let _guard = PopGuard;
        f()
    }
}

/// A surface's context of some type, in a cell so a rebuild can replace it.
#[derive(Clone)]
struct Slot<T>(std::rc::Rc<std::cell::RefCell<T>>);

/// Sets this surface's context of type `T` — what its content wants every widget under it to be able to read
/// without being handed it: which page a panel shows, which screen a chip is on.
///
/// **Written rather than provided, and that is the whole point.** [`provide`] registers a type once per scope
/// and refuses the second attempt, while a surface's scope outlives every build of its content — so a rebuild
/// that provided again would be told it already had one and go on drawing against the context of an edit ago.
pub fn set_context<T: Clone + 'static>(value: T) {
    match try_inject::<Slot<T>>() {
        Some(slot) => *slot.0.borrow_mut() = value,
        None => {
            let _ = provide(Slot(std::rc::Rc::new(std::cell::RefCell::new(value))));
        }
    }
}

/// This surface's context of type `T`, as the latest build left it. `None` before anything set one — a widget
/// built outside a surface, which is every unit test.
pub fn context<T: Clone + 'static>() -> Option<T> {
    try_inject::<Slot<T>>().map(|slot| slot.0.borrow().clone())
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
}
