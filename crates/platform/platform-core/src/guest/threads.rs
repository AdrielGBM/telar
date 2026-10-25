//! Threads a host starts for a guest: the guest hands over a body as a [`ThreadStart`], an entry point of its own and the data it runs on, and the host spawns a thread with its own std that calls it, so no spawn of the guest's std ever runs on a thread of the host's.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

use super::lent::LentBytes;
use super::services::HostServices;
use super::shield;

type Body = Box<dyn FnOnce() + Send>;

/// A body a guest hands its host to run on a thread of the host's: `main(data)`, code of the guest's that owns `data` and frees it, and the thread's name, lent for the call.
#[repr(C)]
pub struct ThreadStart {
    pub(super) name: LentBytes,
    pub(super) main: unsafe extern "C" fn(data: *mut c_void),
    pub(super) data: *mut c_void,
}

/// A thread a host started for a guest, as the guest holds it: the host's own, handed back to that host's `join_thread` or `detach_thread` exactly once.
#[repr(transparent)]
pub struct HostThread {
    pub(super) ptr: *mut c_void,
}

// Only the host's entries read through it, from whichever thread of the guest's hands it back.
unsafe impl Send for HostThread {}

impl HostThread {
    const NULL: Self = Self {
        ptr: std::ptr::null_mut(),
    };

    /// Has the host run `main` on a thread named `name` that its own std spawned: `None` where `services` has no such entry or the host could not start one, and `main` is then dropped unrun.
    ///
    /// # Safety
    ///
    /// `services` must be the table the host handed this guest, still kept by the host.
    pub unsafe fn start(services: &HostServices, name: &str, main: Body) -> Option<Self> {
        let spawn = services.spawn_thread?;
        let data = Box::into_raw(Box::new(main)).cast::<c_void>();
        let start = ThreadStart {
            name: LentBytes::lend(name.as_bytes()),
            main: run_guest_body,
            data,
        };
        let mut thread = Self::NULL;
        if unsafe { spawn(services.data, start, &mut thread) } {
            Some(thread)
        } else {
            drop(unsafe { Box::from_raw(data.cast::<Body>()) });
            None
        }
    }

    /// Waits for the thread to end. A table with no join entry leaves it running on its own.
    ///
    /// # Safety
    ///
    /// `services` must be the table [`start`](Self::start) was handed.
    pub unsafe fn join(self, services: &HostServices) {
        if let Some(join) = services.join_thread {
            unsafe { join(services.data, self) }
        }
    }

    /// Leaves the thread to run on its own.
    ///
    /// # Safety
    ///
    /// As [`join`](Self::join).
    pub unsafe fn detach(self, services: &HostServices) {
        if let Some(detach) = services.detach_thread {
            unsafe { detach(services.data, self) }
        }
    }

    fn of(handle: JoinHandle<()>) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(handle)).cast(),
        }
    }

    unsafe fn into_handle(self) -> Box<JoinHandle<()>> {
        unsafe { Box::from_raw(self.ptr.cast::<JoinHandle<()>>()) }
    }
}

/// The guest's entry point a [`ThreadStart`] names, run on the host's thread: a panic in the body stops here, since unwinding on into the host's frames below would abort the process.
unsafe extern "C" fn run_guest_body(data: *mut c_void) {
    let body = unsafe { Box::from_raw(data.cast::<Body>()) };
    let _ = catch_unwind(AssertUnwindSafe(body));
}

struct GuestBody {
    main: unsafe extern "C" fn(*mut c_void),
    data: *mut c_void,
}

// `data` is a body the guest built `Send`, and only its own `main` reaches it.
unsafe impl Send for GuestBody {}

impl GuestBody {
    fn run(self) {
        unsafe { (self.main)(self.data) }
    }
}

/// The host's `spawn_thread` entry: spawns a thread with this side's std that runs the guest's body, writing its handle to `thread`, or answers `false` with the body never run.
///
/// # Safety
///
/// `start` must be a guest's, lent for the call, and `thread` writable.
pub unsafe extern "C" fn spawn_thread(
    _data: *const c_void,
    start: ThreadStart,
    thread: *mut HostThread,
) -> bool {
    let name = String::from_utf8_lossy(unsafe { start.name.as_slice() }).into_owned();
    let body = GuestBody {
        main: start.main,
        data: start.data,
    };
    shield(false, || {
        match std::thread::Builder::new()
            .name(name)
            .spawn(move || body.run())
        {
            Ok(handle) => {
                unsafe { thread.write(HostThread::of(handle)) };
                true
            }
            Err(error) => {
                tracing::warn!(%error, "a thread a guest asked for could not be started");
                false
            }
        }
    })
}

/// The host's `join_thread` entry: waits for a thread [`spawn_thread`] started to end.
///
/// # Safety
///
/// `thread` must be one this host's `spawn_thread` wrote, never handed back before.
pub unsafe extern "C" fn join_thread(_data: *const c_void, thread: HostThread) {
    let handle = unsafe { thread.into_handle() };
    shield((), || {
        let _ = handle.join();
    });
}

/// The host's `detach_thread` entry: lets a thread [`spawn_thread`] started run on its own.
///
/// # Safety
///
/// As [`join_thread`].
pub unsafe extern "C" fn detach_thread(_data: *const c_void, thread: HostThread) {
    drop(unsafe { thread.into_handle() });
}

#[cfg(test)]
#[path = "threads_test.rs"]
mod tests;
