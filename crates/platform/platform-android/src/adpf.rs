//! Android ADPF: reporting each frame's real work duration so the scheduler can size the clocks for it.

use std::ffi::{c_long, c_void};

mod ffi {
    use std::ffi::c_long;

    #[link(name = "android")]
    unsafe extern "C" {
        pub fn APerformanceHint_getManager() -> *mut std::ffi::c_void;
        pub fn APerformanceHint_createSession(
            manager: *mut std::ffi::c_void,
            thread_ids: *const i32,
            size: usize,
            initial_target_work_duration_ns: c_long,
        ) -> *mut std::ffi::c_void;
        pub fn APerformanceHint_reportActualWorkDuration(
            session: *mut std::ffi::c_void,
            actual_duration_ns: c_long,
        );
        pub fn APerformanceHint_closeSession(session: *mut std::ffi::c_void);
    }
}

// Reports actual per-frame work durations so the scheduler can right-size clocks for the reporting thread. The raw session pointer is not `Send`, so the wrapper stays on the thread that created it and `closeSession` runs there via `Drop`.
/// An ADPF hint session, bound to the thread that created it and closed when it drops.
pub struct AdpfSession {
    session: *mut c_void,
}

impl AdpfSession {
    /// `tid` defaults to the calling thread's kernel TID (SYS_gettid) when None, so the hint must be created on the thread that will report. Returns None when the platform exposes no hint manager or session creation fails.
    pub fn new(target_ns: i64, tid: Option<i32>) -> Option<Self> {
        let session = unsafe {
            let manager = ffi::APerformanceHint_getManager();
            if manager.is_null() {
                return None;
            }
            let tid = tid.unwrap_or_else(|| libc::syscall(libc::SYS_gettid) as i32);
            let s = ffi::APerformanceHint_createSession(manager, &tid, 1, target_ns as c_long);
            if s.is_null() {
                return None;
            }
            s
        };
        Some(Self { session })
    }

    pub fn report(&self, dur_ns: i64) {
        unsafe {
            ffi::APerformanceHint_reportActualWorkDuration(self.session, dur_ns as c_long);
        }
    }
}

impl Drop for AdpfSession {
    fn drop(&mut self) {
        unsafe {
            ffi::APerformanceHint_closeSession(self.session);
        }
    }
}
