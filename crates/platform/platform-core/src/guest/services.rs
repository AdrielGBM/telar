//! The process-global backends a host serves a guest: [`HostServices`], one table of `extern "C"` entries the guest's copy of each store is filled from, so a guest copies to the host's clipboard, keeps preferences in the host's store and sets text in the host's faces instead of finding each of its own stores empty.
//!
//! Every entry for a backend has one shape, a [`Service`]: the table's `data`, a request lent as bytes for the call, and a [`Reply`] the answer is lent to before the entry returns. Each side spells and reads with [`wire_core::Wire`], so what crosses is bytes the lending side owns, and the side reading them keeps a copy in its own allocator. The entries a guest's threads are started through take a [`ThreadStart`] and hand back a [`HostThread`] instead, since what crosses there is code to run rather than a value.

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use wire_core::{Wire, decode, encode};

use super::contract::TypeLayout;
use super::lent::LentBytes;
use super::shield;
use super::threads::{HostThread, ThreadStart};

/// One entry of [`HostServices`]: answers `request` through `reply`, or answers nothing when it cannot read the request or the backend panicked. `None` where the host has no such backend, which the guest takes as its own store left empty.
pub type Service =
    Option<unsafe extern "C" fn(data: *const c_void, request: LentBytes, reply: Reply)>;

/// The entry of [`HostServices`] a guest's threads are started through: has the host spawn a thread with its own std that runs `start`, writing its handle to `thread`, or answers `false` with `start` never run.
pub type SpawnThread = Option<
    unsafe extern "C" fn(data: *const c_void, start: ThreadStart, thread: *mut HostThread) -> bool,
>;

/// The entries of [`HostServices`] that take back a thread `spawn_thread` started, each exactly once: to wait for it to end, or to let it run on its own.
pub type EndThread = Option<unsafe extern "C" fn(data: *const c_void, thread: HostThread)>;

/// Where an entry lends its answer: a sink of the asking side's, which copies the bytes into a buffer of its own before the entry returns. Handed to an entry for that one call, and sent to at most once.
#[repr(C)]
pub struct Reply {
    pub(super) sink: *mut c_void,
    pub(super) put: unsafe extern "C" fn(sink: *mut c_void, answer: LentBytes),
}

impl Reply {
    /// Lends `answer` to the side that asked.
    ///
    /// # Safety
    ///
    /// `self` must be the reply the running entry was handed, sent from inside that call.
    pub unsafe fn send(self, answer: &[u8]) {
        unsafe { (self.put)(self.sink, LentBytes::lend(answer)) }
    }

    /// Runs `ask` with a reply whose answer is kept here, and hands that answer back: `None` when nothing was sent.
    pub fn collect(ask: impl FnOnce(Reply)) -> Option<Vec<u8>> {
        let mut kept: Option<Vec<u8>> = None;
        ask(Reply {
            sink: std::ptr::from_mut(&mut kept).cast(),
            put: keep_answer,
        });
        kept
    }
}

unsafe extern "C" fn keep_answer(sink: *mut c_void, answer: LentBytes) {
    let kept = unsafe { &mut *sink.cast::<Option<Vec<u8>>>() };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        *kept = Some(unsafe { answer.as_slice() }.to_vec());
    }));
}

/// Every process-global backend a host serves the guests it loads, one [`Service`] each, with `data` the host's own and only ever handed back to the host's entries.
///
/// Built by the host from what it installed, valid for as long as the host keeps what `data` points at, which outlives every guest it was handed to. The guest installs one backend of its own per entry that is there, each asking the host through it. An entry's request and answer types are part of the contract: changing one is a [`CONTRACT_VERSION`](super::CONTRACT_VERSION) bump, since the layout hash cannot see it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HostServices {
    pub data: *const c_void,
    /// `()` → `Option<String>`.
    pub clipboard_text: Service,
    /// `String` → `()`.
    pub set_clipboard_text: Service,
    /// `Uri` → `bool`, refused by the host's URI policy as no URI.
    pub open_uri: Service,
    /// `RouteAddress` → `bool`.
    pub open_beside: Service,
    /// `FileDialog` → `Option<PathBuf>`.
    pub open_file: Service,
    /// `FileDialog` → `Vec<PathBuf>`.
    pub open_files: Service,
    /// `FileDialog` → `Option<PathBuf>`.
    pub save_file: Service,
    /// `FileDialog` → `Option<PathBuf>`.
    pub pick_folder: Service,
    /// `String` → `Option<String>`.
    pub stored_preference: Service,
    /// `(String, Option<String>)` → `()`.
    pub store_preference: Service,
    /// `()` → `Option<_>` of the app's name and the directories it is nested under.
    pub app_paths: Service,
    /// `()` → `KeyboardConvention`.
    pub keyboard_convention: Service,
    /// `()` → `Option<LocationFormat>`, as the host's platform installed it.
    pub location_format: Service,
    /// `()` → `Option<String>`, the asset base ending in `/`.
    pub asset_base: Service,
    /// `()` → `LayoutGrid`.
    pub layout_grid: Service,
    /// `()` → `()`, waking the host's loop.
    pub wake_loop: Service,
    /// `()` → `Option<FontSources>`.
    pub font_sources: Service,
    /// `FontConfig` → `Option<FontSources>`.
    pub install_fonts: Service,
    /// `Vec<FontAsset>` → `Option<FontSources>`.
    pub add_font_faces: Service,
    /// `(Vec<FontAsset>, Vec<String>)` → `Option<Result<FontSources, ExclusiveFontsError>>`.
    pub install_only_fonts: Service,
    /// Starts a thread of the host's running a guest's [`ThreadStart`], through [`HostThread::start`].
    pub spawn_thread: SpawnThread,
    /// Waits for a thread `spawn_thread` started, through [`HostThread::join`].
    pub join_thread: EndThread,
    /// Lets a thread `spawn_thread` started run on its own, through [`HostThread::detach`].
    pub detach_thread: EndThread,
}

// `data` is only ever read by the host's entries, which serve from any thread: every backend behind it is `Send + Sync`.
unsafe impl Send for HostServices {}
unsafe impl Sync for HostServices {}

impl HostServices {
    /// A table serving nothing: every store of the guest's stays as its own copy left it.
    pub const NONE: Self = Self {
        data: std::ptr::null(),
        clipboard_text: None,
        set_clipboard_text: None,
        open_uri: None,
        open_beside: None,
        open_file: None,
        open_files: None,
        save_file: None,
        pick_folder: None,
        stored_preference: None,
        store_preference: None,
        app_paths: None,
        keyboard_convention: None,
        location_format: None,
        asset_base: None,
        layout_grid: None,
        wake_loop: None,
        font_sources: None,
        install_fonts: None,
        add_font_faces: None,
        install_only_fonts: None,
        spawn_thread: None,
        join_thread: None,
        detach_thread: None,
    };

    /// Asks `entry` of this table with `request`, reading the answer back as a `T`: `None` where the entry is missing, answered nothing, or answered with bytes that are no `T`.
    ///
    /// # Safety
    ///
    /// `entry` must be one of this table's, and the host must still keep what `data` points at.
    pub unsafe fn ask<T: Wire>(&self, entry: Service, request: &impl Wire) -> Option<T> {
        let entry = entry?;
        let request = encode(request);
        let answer =
            Reply::collect(|reply| unsafe { entry(self.data, LentBytes::lend(&request), reply) })?;
        decode(&answer)
            .inspect_err(|error| tracing::warn!(%error, "the host answered a service with bytes this guest cannot read"))
            .ok()
    }
}

/// Serves one entry on the host's side: reads the request as a `Request`, answers with what `answer` makes of it and the host's `D` behind `data`, and keeps a panic in `answer` on this side, answering nothing.
///
/// # Safety
///
/// `data` must point at the live `D` the table was built over, and `request` and `reply` must be what the entry was called with.
pub unsafe fn serve<D, Request: Wire, Answer: Wire>(
    data: *const c_void,
    request: LentBytes,
    reply: Reply,
    answer: impl FnOnce(&D, Request) -> Answer,
) {
    let host = unsafe { &*data.cast::<D>() };
    let request = unsafe { request.as_slice() };
    shield((), || match decode::<Request>(request) {
        Ok(request) => {
            let answer = encode(&answer(host, request));
            unsafe { reply.send(&answer) };
        }
        Err(error) => {
            tracing::warn!(%error, "a guest asked for a service with bytes this host cannot read")
        }
    });
}

/// The layout [`ContractStamp`](super::ContractStamp) hashes for [`HostServices`].
pub const HOST_SERVICES_LAYOUT: TypeLayout = crate::type_layout!(HostServices {
    data,
    clipboard_text,
    set_clipboard_text,
    open_uri,
    open_beside,
    open_file,
    open_files,
    save_file,
    pick_folder,
    stored_preference,
    store_preference,
    app_paths,
    keyboard_convention,
    location_format,
    asset_base,
    layout_grid,
    wake_loop,
    font_sources,
    install_fonts,
    add_font_faces,
    install_only_fonts,
    spawn_thread,
    join_thread,
    detach_thread,
});

/// The layout [`ContractStamp`](super::ContractStamp) hashes for [`Reply`].
pub const REPLY_LAYOUT: TypeLayout = crate::type_layout!(Reply { sink, put });

#[cfg(test)]
#[path = "services_test.rs"]
mod tests;
