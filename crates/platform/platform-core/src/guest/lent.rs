//! What one side lends the other without giving it away: bytes, and the instances a guest keeps behind opaque handles.

use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;

use wire_core::{Wire, WireError, Writer};

/// Bytes one side lends the other for as long as the contract's rule for them holds: what the host passes into an entry, for that call; what a guest hands back, until the host next calls into that library. The side reading them copies what it keeps and never frees them.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LentBytes {
    pub(super) ptr: *const u8,
    pub(super) len: usize,
}

reactive_local::dropless_thread_local! {
    /// The buffer every answer of an entry run on this thread is spelled into, replaced by the next one: freed by `quiesce`, the last call a host makes.
    pub(super) static ANSWER: ManuallyDrop<RefCell<Writer>> = const { ManuallyDrop::new(RefCell::new(Writer::new())) };
}

impl LentBytes {
    pub const EMPTY: Self = Self {
        ptr: std::ptr::null(),
        len: 0,
    };

    pub fn lend(bytes: &[u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        }
    }

    /// Spells `value` in place of what the last answer on this thread lent, and lends it until the host next calls into this library: what a guest's entry hands back in bytes, once per call.
    pub fn answer<T: Wire>(value: &T) -> Self {
        ANSWER.with(|answer| {
            let mut out = answer.borrow_mut();
            out.clear();
            value.put(&mut out);
            Self::lend(out.as_slice())
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// # Safety
    ///
    /// They must have been lent with [`lend`](Self::lend) or [`answer`](Self::answer), by this side or the other, and the rule they were lent under must still hold: for what the host lent, the call they were passed into has not returned; for what a guest lent, the host has not called into that library since, but to read the pixels of the frame they are.
    pub unsafe fn as_slice<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    /// The value the bytes spell, read into this side's own allocations.
    ///
    /// # Safety
    ///
    /// As [`as_slice`](Self::as_slice).
    pub unsafe fn decode<T: Wire>(self) -> Result<T, WireError> {
        wire_core::decode(unsafe { self.as_slice() })
    }

    /// What the host lent an entry, as the value it spells: a host that spells what its guest cannot read breaks the contract both stamped, so it panics, which the entry reports as its own panic rather than read as something else.
    ///
    /// # Safety
    ///
    /// As [`as_slice`](Self::as_slice).
    pub unsafe fn read<T: Wire>(self) -> T {
        unsafe { self.decode() }.unwrap_or_else(|error| {
            panic!(
                "the host lent bytes this guest cannot read as {}: {error}",
                std::any::type_name::<T>()
            )
        })
    }
}

impl Default for LentBytes {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Frees this thread's answer buffer, which a borrow its spelling panicked out of no longer holds.
pub(super) fn release_answers() {
    ANSWER.with(|answer| {
        if let Ok(mut out) = answer.try_borrow_mut() {
            *out = Writer::new();
        }
    });
}

/// An instance a guest made and keeps, as the host holds it: a pointer it only ever hands back, to the entries of the library that made it, and to that library's release entry exactly once.
///
/// `T` names the instance on the guest's side; the host's copy of that type may be laid out differently, which is harmless because the host never reads through a handle.
#[repr(transparent)]
pub struct Handle<T> {
    pub(super) ptr: *mut c_void,
    _instance: PhantomData<*mut T>,
}

impl<T> Handle<T> {
    pub const NULL: Self = Self {
        ptr: std::ptr::null_mut(),
        _instance: PhantomData,
    };

    /// Moves `instance` behind a handle, in this side's allocator.
    pub fn new(instance: T) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(instance)).cast(),
            _instance: PhantomData,
        }
    }

    pub fn is_null(self) -> bool {
        self.ptr.is_null()
    }

    /// The instance, for the guest's entry that was handed the handle.
    ///
    /// # Safety
    ///
    /// The handle must have come from [`new`](Self::new) in this same image, with this same `T`, and not been [`release`](Self::release)d; and nothing else may reach the instance while the reference lives.
    pub unsafe fn get<'a>(self) -> &'a mut T {
        unsafe { &mut *self.ptr.cast::<T>() }
    }

    /// Drops the instance, in the allocator that made it.
    ///
    /// # Safety
    ///
    /// As [`get`](Self::get), and the handle is never used again.
    pub unsafe fn release(self) {
        drop(unsafe { Box::from_raw(self.ptr.cast::<T>()) });
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Eq for Handle<T> {}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Handle").field(&self.ptr).finish()
    }
}

#[cfg(test)]
#[path = "lent_test.rs"]
mod tests;
