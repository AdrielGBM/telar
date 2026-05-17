use std::any::Any;
use std::cell::RefCell;

use crate::registry::ServiceRegistry;

thread_local! {
    static STACK: RefCell<Vec<ServiceRegistry>> = RefCell::new(vec![ServiceRegistry::new()]);
}

pub fn provide<T: Any + 'static>(service: T) {
    STACK.with(|stack| {
        stack
            .borrow_mut()
            .last_mut()
            .expect("service stack is empty — this is a bug")
            .insert(service);
    });
}

pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    STACK.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find_map(|scope| scope.get::<T>())
            .cloned()
    })
}

pub fn inject<T: Any + Clone + 'static>() -> T {
    try_inject::<T>().unwrap_or_else(|| {
        panic!(
            "service `{}` not found in any scope",
            std::any::type_name::<T>()
        )
    })
}

pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    STACK.with(|stack| {
        let stack = stack.borrow();
        stack.iter().rev().find_map(|scope| scope.get::<T>()).map(f)
    })
}

pub struct Scope(());

impl Scope {
    pub fn new() -> Self {
        STACK.with(|stack| stack.borrow_mut().push(ServiceRegistry::new()));
        Self(())
    }

    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        STACK.with(|stack| stack.borrow_mut().push(ServiceRegistry::new()));
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

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.len() > 1 {
                stack.pop();
            }
        });
    }
}
