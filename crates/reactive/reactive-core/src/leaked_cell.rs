use std::cell::{Cell, Ref, RefCell, RefMut};

/// A thread-local's contents, allocated once per thread and never freed. With no `Drop`, no TLS destructor is registered, so dlclosing a hot-reload dylib on thread exit stays safe.
pub(crate) struct LeakedCell<T>(Cell<*mut RefCell<T>>);

impl<T: Default> LeakedCell<T> {
    pub(crate) fn new() -> Self {
        Self(Cell::new(Box::into_raw(Box::new(RefCell::new(
            T::default(),
        )))))
    }
}

impl<T> LeakedCell<T> {
    pub(crate) fn borrow(&self) -> Ref<'_, T> {
        unsafe { (*self.0.get()).borrow() }
    }

    pub(crate) fn borrow_mut(&self) -> RefMut<'_, T> {
        unsafe { (*self.0.get()).borrow_mut() }
    }
}
