use std::any::Any;
use std::cell::RefCell;

use crate::registry::ServiceRegistry;

thread_local! {
    static STACK: RefCell<Vec<ServiceRegistry>> = RefCell::new(vec![ServiceRegistry::new()]);
}

pub fn provide<T: Any + 'static>(service: T) -> Result<(), crate::registry::ServiceError> {
    STACK.with(|stack| {
        stack
            .borrow_mut()
            .last_mut()
            .expect("service stack is empty — this is a bug")
            .insert(service)
    })
}

pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .and_then(|scope| scope.get::<T>())
            .cloned()
    })
}

pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    STACK.with(|stack| {
        let stack = stack.borrow();
        stack.last().and_then(|scope| scope.get::<T>()).map(f)
    })
}

pub struct Scope(());

impl Scope {
    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let mut new_scope = ServiceRegistry::new();
            if let Some(parent) = stack.last() {
                new_scope.merge_from(parent);
            }
            stack.push(new_scope);
        });
        struct PopGuard;
        impl Drop for PopGuard {
            fn drop(&mut self) {
                STACK.with(|stack| {
                    let mut stack = stack.borrow_mut();
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
