//! A waker that crosses as data and function pointers rather than as a [`RedrawWaker`], whose `Arc` would be freed by the allocator of whichever side dropped it last.

use std::ffi::c_void;

use super::shield;
use crate::RedrawWaker;

/// One reference to a waker kept by the side that made it: `wake`, `clone` and `release` are that side's code, so the data is only ever touched, copied and freed where it was allocated.
///
/// Whoever holds a `CWaker` owns that one reference: dropping it releases it, cloning it asks the maker for another. The maker's library must stay mapped while any reference lives, which holds for the host's wakers by construction, since a guest is always unloaded first.
#[repr(C)]
pub struct CWaker {
    pub(super) data: *const c_void,
    pub(super) wake: unsafe extern "C" fn(*const c_void),
    pub(super) clone: unsafe extern "C" fn(*const c_void) -> *const c_void,
    pub(super) release: unsafe extern "C" fn(*const c_void),
}

// The data is a `RedrawWaker`, which is `Send` and `Sync`, and only the maker's own functions ever reach it.
unsafe impl Send for CWaker {}
unsafe impl Sync for CWaker {}

impl CWaker {
    /// Lends `waker` as one reference, made and later freed on this side.
    pub fn new(waker: RedrawWaker) -> Self {
        Self {
            data: Box::into_raw(Box::new(waker)).cast_const().cast(),
            wake: wake_redraw,
            clone: clone_redraw,
            release: release_redraw,
        }
    }

    pub fn wake(&self) {
        unsafe { (self.wake)(self.data) }
    }

    /// A [`RedrawWaker`] of the receiving side's own that wakes through this one, for code that takes one.
    pub fn into_redraw_waker(self) -> RedrawWaker {
        RedrawWaker::new(move || self.wake())
    }
}

impl Clone for CWaker {
    fn clone(&self) -> Self {
        Self {
            data: unsafe { (self.clone)(self.data) },
            wake: self.wake,
            clone: self.clone,
            release: self.release,
        }
    }
}

impl Drop for CWaker {
    fn drop(&mut self) {
        unsafe { (self.release)(self.data) }
    }
}

impl std::fmt::Debug for CWaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CWaker").field("data", &self.data).finish()
    }
}

unsafe extern "C" fn wake_redraw(data: *const c_void) {
    let waker = unsafe { &*data.cast::<RedrawWaker>() };
    shield((), || waker.wake());
}

// A clone that panicked still answers with a reference the other side may wake and release, one that wakes nothing.
unsafe extern "C" fn clone_redraw(data: *const c_void) -> *const c_void {
    let waker = unsafe { &*data.cast::<RedrawWaker>() };
    let clone = shield(None, || Some(waker.clone())).unwrap_or_else(|| RedrawWaker::new(|| {}));
    Box::into_raw(Box::new(clone)).cast_const().cast()
}

unsafe extern "C" fn release_redraw(data: *const c_void) {
    let waker = unsafe { Box::from_raw(data.cast_mut().cast::<RedrawWaker>()) };
    shield((), || drop(waker));
}

#[cfg(test)]
#[path = "waker_test.rs"]
mod tests;
