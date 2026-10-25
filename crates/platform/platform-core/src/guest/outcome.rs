//! What every entry of the contract returns in place of unwinding: whether its body ran to the end, and if it panicked, what the panic said, lent as bytes.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use super::lent::LentBytes;
use super::{GuestPanic, GuestResult, guest_call};

reactive_local::dropless_thread_local! {
    /// The panic report a guest lends, one per thread that calls into it: replaced by the next panic on that thread, and freed by `quiesce`, the last call a host makes, which is the one call a guest that panicked still gets.
    pub(super) static REPORT: ManuallyDrop<RefCell<Option<GuestPanic>>> = const { ManuallyDrop::new(RefCell::new(None)) };
}

/// How an entry's call ended. Its outputs, when it has any, are written through pointers the host passed in, and are read only when it [returned](Self::RETURNED).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CallOutcome {
    pub(super) status: u32,
    pub(super) message: LentBytes,
    pub(super) location: LentBytes,
}

impl CallOutcome {
    pub const RETURNED: u32 = 0;
    pub const PANICKED: u32 = 1;

    pub const OK: Self = Self {
        status: Self::RETURNED,
        message: LentBytes::EMPTY,
        location: LentBytes::EMPTY,
    };

    pub fn status(&self) -> u32 {
        self.status
    }

    /// The host's reading of the outcome, owned on its side, for [`Guest::call`](super::Guest::call) to stop calling a guest that panicked. A status this contract does not know is taken as a panic, since what the guest did is unknown.
    ///
    /// # Safety
    ///
    /// It must be the outcome a guest entry just returned, read before this thread calls anything else in that library: the report is lent until then.
    pub unsafe fn into_result(self) -> GuestResult<()> {
        match self.status {
            Self::RETURNED => Ok(()),
            Self::PANICKED => {
                let text = |bytes: LentBytes| {
                    String::from_utf8_lossy(unsafe { bytes.as_slice() }).into_owned()
                };
                Err(GuestPanic {
                    message: text(self.message),
                    location: (!self.location.is_empty()).then(|| text(self.location)),
                })
            }
            status => Err(GuestPanic {
                message: format!(
                    "the guest answered with status {status}, which this contract does not know"
                ),
                location: None,
            }),
        }
    }
}

/// Runs one entry's body on the guest's side, so a panic stops here and is reported in the outcome instead of unwinding into the host.
pub fn guest_entry(call: impl FnOnce()) -> CallOutcome {
    match guest_call(call) {
        Ok(()) => CallOutcome::OK,
        Err(panic) => lend_report(panic),
    }
}

fn lend_report(panic: GuestPanic) -> CallOutcome {
    REPORT.with(|report| {
        let mut report = report.borrow_mut();
        let panic = report.insert(panic);
        CallOutcome {
            status: CallOutcome::PANICKED,
            message: LentBytes::lend(panic.message.as_bytes()),
            location: panic
                .location
                .as_deref()
                .map_or(LentBytes::EMPTY, |location| {
                    LentBytes::lend(location.as_bytes())
                }),
        }
    })
}

/// Frees the report this thread lent last, which its host has read.
pub(super) fn release_report() {
    REPORT.with(|report| {
        if let Ok(mut report) = report.try_borrow_mut() {
            *report = None;
        }
    });
}

#[cfg(test)]
#[path = "outcome_test.rs"]
mod tests;
