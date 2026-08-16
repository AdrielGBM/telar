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
