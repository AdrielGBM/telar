//! The process-global backends a host serves a guest: [`HostServices`], one table of `extern "C"` entries the guest's copy of each store is filled from, so a guest copies to the host's clipboard, keeps preferences in the host's store and sets text in the host's faces instead of finding each of its own stores empty.
//!
//! Every entry for a backend has one shape, a [`Service`]: the table's `data`, a request lent as bytes for the call, and a [`Reply`] the answer is lent to before the entry returns. Both are typed by the entry, so a host serving one type and a guest asking another do not compile, and a [`ContractStamp`](super::ContractStamp) holds the shape of every request and answer. Each side spells and reads with [`wire_core::Wire`], so what crosses is bytes the lending side owns, and the side reading them keeps a copy in its own allocator. The entries a guest's threads are started through take a [`ThreadStart`] and hand back a [`HostThread`] instead, since what crosses there is code to run rather than a value.
//!
//! The backends whose values this crate cannot name, file dialogs, the app's paths and the font database among them, are the `upper` table at the end of [`HostServices`], which the layer above declares as an [`UpperServices`].

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use geometry_core::LayoutGrid;
use semantics_core::{RouteAddress, Uri};
use wire_core::{Wire, decode, encode};

use super::contract::{ArgShape, TypeLayout};
use super::lent::LentBytes;
use super::shield;
use super::threads::{HostThread, ThreadStart};
use crate::{KeyboardConvention, LocationFormat};

/// One entry of [`HostServices`]: answers a `Request` through a [`Reply`] with an `Answer`, or answers nothing when it cannot read the request or the backend panicked. `None` where the host has no such backend, which the guest takes as its own store left empty.
pub type Service<Request, Answer> = Option<
    unsafe extern "C" fn(data: *const c_void, request: LentBytes<Request>, reply: Reply<Answer>),
>;

/// The entry of [`HostServices`] a guest's threads are started through: has the host spawn a thread with its own std that runs `start`, writing its handle to `thread`, or answers `false` with `start` never run.
pub type SpawnThread = Option<
    unsafe extern "C" fn(data: *const c_void, start: ThreadStart, thread: *mut HostThread) -> bool,
>;

/// The entries of [`HostServices`] that take back a thread `spawn_thread` started, each exactly once: to wait for it to end, or to let it run on its own.
pub type EndThread = Option<unsafe extern "C" fn(data: *const c_void, thread: HostThread)>;

/// Where an entry lends its `Answer`: a sink of the asking side's, which copies the bytes into a buffer of its own before the entry returns. Handed to an entry for that one call, and sent to at most once.
#[repr(C)]
pub struct Reply<Answer> {
    pub(super) sink: *mut c_void,
    pub(super) put: unsafe extern "C" fn(sink: *mut c_void, answer: LentBytes<Answer>),
}

impl<Answer: Wire> Reply<Answer> {
    /// Lends `answer` to the side that asked.
    ///
    /// # Safety
    ///
    /// `self` must be the reply the running entry was handed, sent from inside that call.
    pub unsafe fn send(self, answer: &Answer) {
        let bytes = encode(answer);
        unsafe { (self.put)(self.sink, LentBytes::lend(&bytes)) }
    }

    /// Runs `ask` with a reply whose answer is kept here, and hands that answer back as it was spelled: `None` when nothing was sent.
    pub fn collect(ask: impl FnOnce(Self)) -> Option<Vec<u8>> {
        let mut kept: Option<Vec<u8>> = None;
        ask(Self {
            sink: std::ptr::from_mut(&mut kept).cast(),
            put: keep_answer::<Answer>,
        });
        kept
    }
}

unsafe extern "C" fn keep_answer<Answer>(sink: *mut c_void, answer: LentBytes<Answer>) {
    let kept = unsafe { &mut *sink.cast::<Option<Vec<u8>>>() };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        *kept = Some(unsafe { answer.as_slice() }.to_vec());
    }));
}

/// The services of the layer above this crate, the `upper` table at the end of [`HostServices`]: a `repr(C)` table of [`Service`]s whose requests and answers are types this crate does not link, declared once by that layer for every boundary.
pub trait UpperServices: ArgShape + Copy + std::fmt::Debug + 'static {
    /// A table serving nothing.
    const NONE: Self;
    /// What [`ContractStamp`](super::ContractStamp) hashes for the table, its fields' shapes included.
    const LAYOUT: TypeLayout;
}

/// Every process-global backend a host serves the guests it loads, one [`Service`] each, with `data` the host's own and only ever handed back to the host's entries, the `upper` table's included.
///
/// Built by the host from what it installed, valid for as long as the host keeps what `data` points at, which outlives every guest it was handed to. The guest installs one backend of its own per entry that is there, each asking the host through it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostServices<Upper> {
    pub data: *const c_void,
    pub clipboard_text: Service<(), Option<String>>,
    pub set_clipboard_text: Service<String, ()>,
    /// Refused by the host's URI policy as no URI.
    pub open_uri: Service<Uri, bool>,
    pub open_beside: Service<RouteAddress, bool>,
    pub stored_preference: Service<String, Option<String>>,
    pub store_preference: Service<(String, Option<String>), ()>,
    pub keyboard_convention: Service<(), KeyboardConvention>,
    /// As the host's platform installed it.
    pub location_format: Service<(), Option<LocationFormat>>,
    /// The asset base, ending in `/`.
    pub asset_base: Service<(), Option<String>>,
    pub layout_grid: Service<(), LayoutGrid>,
    /// Wakes the host's loop.
    pub wake_loop: Service<(), ()>,
    /// Starts a thread of the host's running a guest's [`ThreadStart`], through [`HostThread::start`].
    pub spawn_thread: SpawnThread,
    /// Waits for a thread `spawn_thread` started, through [`HostThread::join`].
    pub join_thread: EndThread,
    /// Lets a thread `spawn_thread` started run on its own, through [`HostThread::detach`].
    pub detach_thread: EndThread,
    pub upper: Upper,
}

// `data` is only ever read by the host's entries, which serve from any thread: every backend behind it is `Send + Sync`.
unsafe impl<Upper: Send> Send for HostServices<Upper> {}
unsafe impl<Upper: Sync> Sync for HostServices<Upper> {}

impl<Upper: UpperServices> HostServices<Upper> {
    /// A table serving nothing: every store of the guest's stays as its own copy left it.
    pub const NONE: Self = Self {
        data: std::ptr::null(),
        clipboard_text: None,
        set_clipboard_text: None,
        open_uri: None,
        open_beside: None,
        stored_preference: None,
        store_preference: None,
        keyboard_convention: None,
        location_format: None,
        asset_base: None,
        layout_grid: None,
        wake_loop: None,
        spawn_thread: None,
        join_thread: None,
        detach_thread: None,
        upper: Upper::NONE,
    };

    /// The layout [`ContractStamp`](super::ContractStamp) hashes for the table, every entry's request and answer shape with it.
    pub const LAYOUT: TypeLayout = crate::type_layout!(HostServices<Upper> {
        data,
        clipboard_text,
        set_clipboard_text,
        open_uri,
        open_beside,
        stored_preference,
        store_preference,
        keyboard_convention,
        location_format,
        asset_base,
        layout_grid,
        wake_loop,
        spawn_thread,
        join_thread,
        detach_thread,
        upper,
    });
}

impl<Upper> HostServices<Upper> {
    /// Asks `entry`, of this table or of its `upper` one, with `request`, reading the answer back as the entry's `Answer`: `None` where the entry is missing, answered nothing, or answered with bytes that are no `Answer`.
    ///
    /// # Safety
    ///
    /// `entry` must be one of this table's, and the host must still keep what `data` points at.
    pub unsafe fn ask<Request: Wire, Answer: Wire>(
        &self,
        entry: Service<Request, Answer>,
        request: &Request,
    ) -> Option<Answer> {
        let entry = entry?;
        let request = encode(request);
        let answer = Reply::<Answer>::collect(|reply| unsafe {
            entry(self.data, LentBytes::lend(&request), reply)
        })?;
        decode(&answer)
            .inspect_err(|error| tracing::warn!(%error, "the host answered a service with bytes this guest cannot read"))
            .ok()
    }
}

impl<Upper: UpperServices> ArgShape for HostServices<Upper> {
    const ARG_SHAPE: u64 = Self::LAYOUT.shape;
}

/// Serves one entry on the host's side: reads the request as the entry's `Request`, answers with what `answer` makes of it and the host's `D` behind `data`, and keeps a panic in `answer` on this side, answering nothing.
///
/// # Safety
///
/// `data` must point at the live `D` the table was built over, and `request` and `reply` must be what the entry was called with.
pub unsafe fn serve<D, Request: Wire, Answer: Wire>(
    data: *const c_void,
    request: LentBytes<Request>,
    reply: Reply<Answer>,
    answer: impl FnOnce(&D, Request) -> Answer,
) {
    let host = unsafe { &*data.cast::<D>() };
    let request = unsafe { request.as_slice() };
    shield((), || match decode::<Request>(request) {
        Ok(request) => unsafe { reply.send(&answer(host, request)) },
        Err(error) => {
            tracing::warn!(%error, "a guest asked for a service with bytes this host cannot read")
        }
    });
}

/// The layout [`ContractStamp`](super::ContractStamp) hashes for [`Reply`], whose answer each entry types.
pub const REPLY_LAYOUT: TypeLayout = crate::type_layout!(Reply<()> { sink, put });

/// The `upper` table of a boundary whose layer above serves one thing nobody asks for, for the tests of this crate.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct NoUpper {
    pub unasked: Service<(), ()>,
}

#[cfg(test)]
impl ArgShape for NoUpper {
    const ARG_SHAPE: u64 = wire_core::shape_named("NoUpper");
}

#[cfg(test)]
impl UpperServices for NoUpper {
    const NONE: Self = Self { unasked: None };
    const LAYOUT: TypeLayout = crate::type_layout!(NoUpper { unasked });
}

#[cfg(test)]
#[path = "services.test.rs"]
mod tests;
