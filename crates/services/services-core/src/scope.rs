use std::any::Any;
use std::cell::{Cell, RefCell};

use crate::registry::ServiceRegistry;

type Stack = *mut RefCell<Vec<ServiceRegistry>>;

// The live stack and the ambient one it started as. Both, because "restore what was active" and "activate the stack of a surface-less caller" are different questions and only the second needs the saved ambient.
#[derive(Clone, Copy)]
struct StackSlot {
    live: Stack,
    ambient: Stack,
}

thread_local! {
    // The active service stack sits behind a swappable pointer (the same idiom as platform-core's
    // `WindowCommandContext`): the cell holds a raw pointer and has no Drop, so no TLS destructor runs on
    // thread exit. A surface activates its own stack via `ServiceContext::enter`; single-window apps run
    // against the leaked ambient stack unchanged. The base box is leaked on purpose.
    static STACK: Cell<StackSlot> = {
        let ambient: Stack = Box::into_raw(Box::new(RefCell::new(vec![ServiceRegistry::new()])));
        Cell::new(StackSlot { live: ambient, ambient })
    };
}

fn with_stack<R>(f: impl FnOnce(&RefCell<Vec<ServiceRegistry>>) -> R) -> R {
    // SAFETY: the pointer always addresses a live `RefCell<Vec<ServiceRegistry>>` (the leaked ambient stack, or
    // a `ServiceContext` box that outlives every guard pointing the cell at it); the borrow is released before
    // the closure returns.
    STACK.with(|cell| unsafe { f(&*cell.get().live) })
}

fn swap_live(next: Stack) -> Stack {
    STACK.with(|cell| {
        let mut slot = cell.get();
        let prev = std::mem::replace(&mut slot.live, next);
        cell.set(slot);
        prev
    })
}

pub fn provide<T: Any + 'static>(service: T) -> Result<(), crate::registry::ServiceError> {
    with_stack(|stack| {
        stack
            .borrow_mut()
            .last_mut()
            .expect("service stack is empty — this is a bug")
            .insert(service)
    })
}

pub fn try_inject<T: Any + Clone + 'static>() -> Option<T> {
    with_stack(|stack| {
        stack
            .borrow()
            .last()
            .and_then(|scope| scope.get::<T>())
            .cloned()
    })
}

pub fn with_service<T: Any + 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    with_stack(|stack| {
        let stack = stack.borrow();
        stack.last().and_then(|scope| scope.get::<T>()).map(f)
    })
}

pub struct Scope(());

impl Scope {
    pub fn with<R>(f: impl FnOnce() -> R) -> R {
        with_stack(|stack| {
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
                with_stack(|stack| {
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

/// A per-surface service stack. Under M3 many surfaces share one UI thread, so each surface owns its own
/// stack; the runner activates it with [`ServiceContext::enter`] around the surface's build/event/frame (and
/// the reactive flush re-enters it for that surface's effects), so `provide`/`try_inject` resolve against the
/// surface whose context is active — the generic per-surface context primitive (theme/locale/config/DI).
pub struct ServiceContext {
    ptr: *mut RefCell<Vec<ServiceRegistry>>,
}

impl ServiceContext {
    pub fn new() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(RefCell::new(vec![ServiceRegistry::new()]))),
        }
    }

    #[must_use = "the surface's services are only active while this guard is alive"]
    pub fn enter(&self) -> ServiceGuard {
        ServiceGuard {
            prev: swap_live(self.ptr),
        }
    }

    /// Activates the ambient stack — the one a caller that never built a surface resolves against. See
    /// `Surface::enter_ambient` in `ui-core` for why the reactive flush needs it.
    #[must_use = "the ambient services are only active while this guard is alive"]
    pub fn enter_ambient() -> ServiceGuard {
        let ambient = STACK.with(|cell| cell.get().ambient);
        ServiceGuard {
            prev: swap_live(ambient),
        }
    }
}

impl Default for ServiceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ServiceContext {
    fn drop(&mut self) {
        // A matching guard has restored the previous stack, so this box is not the live pointer.
        unsafe { drop(Box::from_raw(self.ptr)) };
    }
}

#[must_use = "the surface's services are only active while this guard is alive"]
pub struct ServiceGuard {
    prev: *mut RefCell<Vec<ServiceRegistry>>,
}

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        swap_live(self.prev);
    }
}
