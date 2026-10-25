//! The entries every table starts with, called the same way whatever the guest is: [`Lifecycle`], and the [`Entries`] trait a host drives any table through.

use std::sync::{Mutex, PoisonError};

use reactive_local::thread_exit;

use super::clock::{Epoch, Timestamp};
use super::contract::{ContractStamp, TypeLayout};
use super::lent::release_answers;
use super::outcome::{CallOutcome, guest_entry, release_report};
use super::services::HostServices;
use super::surface::SurfaceReport;

/// The entries a host calls on every guest, in this order: `loaded` and `adopt_epoch`, for a library it will unmap, then `install_services`, before anything else; once every instance is released, `unloading`, unless the guest panicked, and `quiesce`, always, right before the library is unmapped. Entries a process serves itself share its epoch and are never unmapped.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Lifecycle {
    pub loaded: unsafe extern "C" fn() -> CallOutcome,
    pub adopt_epoch: unsafe extern "C" fn(now: Timestamp) -> CallOutcome,
    pub install_services: unsafe extern "C" fn(services: HostServices) -> CallOutcome,
    pub unloading: unsafe extern "C" fn() -> CallOutcome,
    pub quiesce: unsafe extern "C" fn() -> CallOutcome,
}

impl Lifecycle {
    /// The lifecycle a guest serves, with `install_services` filling its stores from the host's table: the one entry each boundary writes itself, since what a guest installs beyond those stores, its shaper's font database, is in a crate this one does not link.
    pub const fn serving(
        install_services: unsafe extern "C" fn(services: HostServices) -> CallOutcome,
    ) -> Self {
        Self {
            loaded,
            adopt_epoch,
            install_services,
            unloading,
            quiesce,
        }
    }
}

/// The layout [`ContractStamp`] hashes for [`Lifecycle`], each entry with its signature.
pub(super) const LIFECYCLE_LAYOUT: TypeLayout = TypeLayout {
    name: "Lifecycle",
    size: std::mem::size_of::<Lifecycle>(),
    align: std::mem::align_of::<Lifecycle>(),
    fields: &[
        ("loaded: fn()", std::mem::offset_of!(Lifecycle, loaded)),
        (
            "adopt_epoch: fn(Timestamp)",
            std::mem::offset_of!(Lifecycle, adopt_epoch),
        ),
        (
            "install_services: fn(HostServices)",
            std::mem::offset_of!(Lifecycle, install_services),
        ),
        (
            "unloading: fn()",
            std::mem::offset_of!(Lifecycle, unloading),
        ),
        ("quiesce: fn()", std::mem::offset_of!(Lifecycle, quiesce)),
    ],
};

/// A boundary's table of entries, which starts with its [`Lifecycle`] and carries the [`SurfaceReport`] of the instances it builds, as [`entry_table!`](crate::entry_table) declares one: what [`open_guest`](super::open_guest) copies out of a library, and what a host drives any guest through.
///
/// # Safety
///
/// `Self` is a `repr(C)` table of `extern "C"` entries that follow the contract, laid out as [`LAYOUT`](Self::LAYOUT) says, and a library that exports it as [`SYMBOL`](Self::SYMBOL) exports [`CONTRACT`](Self::CONTRACT) beside it.
pub unsafe trait Entries: Copy + 'static {
    /// The instance on the guest's side whose surface [`report`](Self::report) reads, behind the handles its host holds.
    type Surface;
    /// The symbol the table is exported as, ending in a nul.
    const SYMBOL: &'static [u8];
    const LAYOUT: TypeLayout;
    /// The stamp a library exports beside the table, and its host refuses it without.
    const CONTRACT: ContractStamp;

    fn lifecycle(&self) -> &Lifecycle;

    fn report(&self) -> &SurfaceReport<Self::Surface>;
}

static HOST_EPOCH: Mutex<Option<Epoch>> = Mutex::new(None);

/// The host's epoch as this copy adopted it, which every [`Timestamp`] either side sends counts from.
///
/// A copy no host has handed one, such as a host serving entries of its own, takes the moment it first asks, and both sides read that one: its timestamps still lie exactly as far apart as the instants they stamp, where reading each as the moment it arrives would collapse every duration between them.
pub fn host_epoch() -> Epoch {
    *HOST_EPOCH
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(|| Epoch::adopt(Timestamp::default()))
}

extern "C" fn loaded() -> CallOutcome {
    guest_entry(thread_exit::leave_to_host)
}

extern "C" fn adopt_epoch(now: Timestamp) -> CallOutcome {
    guest_entry(|| {
        *HOST_EPOCH.lock().unwrap_or_else(PoisonError::into_inner) = Some(Epoch::adopt(now));
    })
}

extern "C" fn unloading() -> CallOutcome {
    guest_entry(thread_exit::release_guest)
}

/// Joins every thread still running this copy's code, then frees what the entries lent this thread, a panic's report included: the last call a host makes, whether or not the guest panicked, so nothing else would.
extern "C" fn quiesce() -> CallOutcome {
    guest_entry(|| {
        thread_exit::quiesce();
        release_answers();
        release_report();
    })
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
