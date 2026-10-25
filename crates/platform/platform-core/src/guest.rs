//! A dylib that carries its own copy of the runtime, and the boundary between it and the host that loads it: hot reload swapping a whole application in, and an embedded guest compositing into the host's frame.
//!
//! Each side links its own copy of std, so a panic unwinding out of one into the other is a foreign exception there, and the only thing a Rust runtime does with one of those is abort the process. So nothing unwinds across: every entry runs its body under [`guest_entry`] and reports a panic in the [`CallOutcome`] it returns, the host makes every call through a [`Guest`] that stops calling once one has panicked, and host code the guest is handed catches its own panics with [`shield`] before they reach a frame of the guest's.
//!
//! # The contract
//!
//! Hot reload and embed load their guests through one hand-written `repr(C)` contract, so that two builds with their own allocators, features or compilers can only meet on types whose layout C fixes. Its rules, which every entry of either boundary follows:
//!
//! - **Nothing owned crosses, and each side frees only what it allocated.** What crosses is scalars, the building blocks below and bytes; a rich value (an event, preferences, a draw list, a string) is spelled with [`wire_core::Wire`] into a buffer the spelling side owns. No `repr(Rust)` type crosses, so neither does an `Arc`, a `Box`, a `Vec` or a trait object.
//! - **Lent bytes** ([`LentBytes`]): what the host passes into an entry is valid for that call only. What a guest hands back is lent until the host next calls into that library, through [`LentBytes::answer`], which spells every answer into one buffer per thread. A frame is the one exception: it stays lent while the host reads the pixels of the images it draws through the entry that lends them, each valid until the next call. The reader copies what it keeps and never frees it.
//! - **Every entry is `extern "C"` and returns a [`CallOutcome`]**, writing its outputs through pointers the host passed in. The guest runs the body under [`guest_entry`], which catches a panic on its side and reports it as a status with the message and location lent as bytes, under the rule above. The host reads it with [`CallOutcome::into_result`] inside [`Guest::call`], which stops calling once a guest has panicked.
//! - **Callbacks catch on the side whose code they are, and report nothing.** [`Reply`]'s `put`, a [`ThreadStart`]'s `main` and a [`CWaker`]'s functions run one side's code under the other's frames, so each catches its own panic before it returns, and one that answers with a value falls back to one that is always sound: a waker that wakes nothing.
//! - **Instances are opaque [`Handle`]s.** The host only passes a handle back to the library that made it, never reads through it, and hands it to that library's release entry exactly once. Once the guest has panicked the host calls nothing of it but `quiesce`, and its instances go with the library.
//! - **Every entry table starts with one [`Lifecycle`]**, declared with [`entry_table!`](crate::entry_table) as an [`Entries`] table: `loaded`, `adopt_epoch` and `install_services` run before anything else, and `unloading` then `quiesce` after every instance is released, the last calls before the library is unmapped.
//! - **Every entry table carries one [`SurfaceReport`]** for the instances it builds, the same for both boundaries: what a screen reader and the input method are told about the surface an instance draws, in that surface's own coordinates and ids, a screen reader's action on it, and its keyboard behind the one focus stop the host keeps for it. The host places all of it where its frame shows the surface.
//! - **Wakers cross as a [`CWaker`]**: data plus `wake`, `clone` and `release` pointing into the image that made it. Whoever holds one owns one reference and drops it before that image is unloaded.
//! - **A frame's context crosses as a [`FrameCtx`]** in place of an `AppCtx`: the redraw the guest asks for written back, the host's loop waker borrowed for the call, and the window's raw handles as bytes.
//! - **Time crosses as a [`Timestamp`]**, signed nanoseconds since an [`Epoch`] the host sets at load and the guest adopts through `adopt_epoch`, read back with [`host_epoch`]; durations and every other rich value cross as bytes.
//! - **Process-global backends cross as one [`HostServices`] table**: the clipboard, the URI opener, file dialogs, the preference store, the app's paths and name, the keyboard convention, the location format, the asset base, the layout grid, the loop wake, the font database and the threads the guest starts. The host builds it over what it installed and keeps it until the guest is unloaded, or with the library for the rest of the process when the guest's threads could not be joined; the guest installs a backend of its own per entry, which asks the host through it, and frees those when it is unloaded like any other static of its own.
//! - **The host starts the guest's threads.** std runs its spawn hooks on the thread that spawns, through a thread-local whose destructor keeps the library that reached it mapped until that thread exits, and the host's UI thread never does. So every thread the runtime starts goes through `reactive_local::threads`, which a guest routes to the table's `spawn_thread`: it hands over an entry point of its own and the data it runs on as a [`ThreadStart`], the host spawns a thread with its own std that calls it, and hands back a [`HostThread`] the guest joins or lets go through the table exactly once. A guest joins every thread still running its code before its host unmaps it, through `quiesce`.
//! - **One [`CONTRACT_VERSION`]** for both boundaries, and a [`ContractStamp`] that names the [`GuestKind`] a library was built as and hashes the layout of every `repr(C)` type involved with [`wire_core::WIRE_VERSION`]. A guest exports it with [`export_contract_stamp!`](crate::export_contract_stamp), and [`open_guest`], the one loader, refuses a library whose stamp differs before reading anything else of it. Loading stays `unsafe`: a matching stamp proves the builds agree, not that the code is sound.
//!
//! What a guest draws crosses as a draw list (`renderer_core::wire`), with two limits by design: a link whose scheme the host's URI policy refuses arrives as no link while its box still draws, and an image held in a GPU texture the guest made arrives as an image of its size with no pixels.
//!
//! One limit the contract cannot lift: app code, or a crate it links, that calls `std::thread::spawn` itself from a thread of the host's still spawns with the guest's std, and keeps that build mapped until the thread exits, then until the next `dlclose` of a guest; on the UI thread, for the rest of the session. App code starts its threads with `telar::threads::spawn` instead, which goes through the host, until std's `Builder::no_hooks` (rust-lang/rust#132951) stabilises and lets a guest's own spawns skip the hooks.

mod clock;
mod contract;
mod frame_ctx;
mod lent;
mod lifecycle;
mod outcome;
mod services;
mod surface;
mod threads;
mod waker;

pub use clock::{Epoch, Timestamp};
#[doc(hidden)]
pub use contract::check_entry_table;
pub use contract::{
    BUILDING_BLOCKS, CONTRACT_SYMBOL, CONTRACT_VERSION, ContractMismatch, ContractStamp, GuestKind,
    TypeLayout,
};
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
pub use contract::{OpenGuestError, open_guest};
pub use frame_ctx::FrameCtx;
pub(crate) use frame_ctx::RawHandles;
pub use lent::{Handle, LentBytes};
pub use lifecycle::{Entries, Lifecycle, host_epoch};
pub use outcome::{CallOutcome, guest_entry};
pub use services::{
    EndThread, HOST_SERVICES_LAYOUT, HostServices, REPLY_LAYOUT, Reply, Service, SpawnThread, serve,
};
pub use surface::{ReportsSurface, SurfaceReport};
pub use threads::{HostThread, ThreadStart, detach_thread, join_thread, spawn_thread};
pub use waker::CWaker;

use std::any::Any;
use std::cell::{Cell, OnceCell};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

/// Opens `path` with the loading policy a guest needs: resolve every symbol now, and keep the guest's symbols **local** so its own copy of the runtime never interposes on the host's.
///
/// Two runtimes in one process is the premise of every model that loads one, and each is only sound while the two stay apart. A global-scope load lets the guest's copy of a `thread_local` accessor answer for the host's, and what that looks like is not a crash: it is a registry the host fills and the guest never reads, silently, for the life of the process.
///
/// # Safety
///
/// As [`open_guest`], which is the only caller.
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
unsafe fn open(path: &std::path::Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(unix)]
    {
        unsafe {
            libloading::os::unix::Library::open(
                Some(path.as_os_str()),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            )
            .map(libloading::Library::from)
        }
    }
    #[cfg(not(unix))]
    {
        unsafe { libloading::Library::new(path) }
    }
}

/// Opens a private copy of the library at `path`, so every load is a fresh instance of it.
///
/// dlopen caches by path and by (device, inode), so loading a library again while an earlier load of it is still mapped, a rebuilt one written in place or one that panicked and has not been dropped yet, hands back the mapping already there, statics and all. The copy has a path no earlier load had; unlinking it after the open is safe, since the mapping keeps the inode alive.
///
/// A guest dropped afterwards is unmapped by `dlclose`, except that on glibc a thread-local with a destructor keeps its library mapped until the thread exits. No thread-local of the runtime has one, the panic slot behind [`guest_entry`] included, a guest told by its `loaded` entry that it will be unloaded registers none of its own either, and its threads are started by its host, so std's spawn hooks never reach one of its own on a host thread.
///
/// # Safety
///
/// As [`open_guest`], which is the only caller.
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
unsafe fn open_unique(
    path: &std::path::Path,
) -> Result<libloading::Library, Box<dyn std::error::Error>> {
    let unique = unique_copy_path(path);
    std::fs::copy(path, &unique)?;
    let library = unsafe { open(&unique) };
    let _ = std::fs::remove_file(&unique);
    Ok(library?)
}

#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
fn unique_copy_path(path: &std::path::Path) -> std::path::PathBuf {
    static COPIES: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!(
        ".guest-{}-{}-{nanos}",
        std::process::id(),
        COPIES.fetch_add(1, Ordering::Relaxed)
    );
    let unique = path.with_file_name(name);
    match path.extension() {
        Some(extension) => {
            let mut with_extension = unique.into_os_string();
            with_extension.push(".");
            with_extension.push(extension);
            with_extension.into()
        }
        None => unique,
    }
}

const fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
        index += 1;
    }
    hash
}

/// A panic the guest caught at its own edge: what it said and where, as the panic hook saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestPanic {
    pub message: String,
    pub location: Option<String>,
}

impl fmt::Display for GuestPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(location) => write!(f, "panicked at {location}:\n{}", self.message),
            None => write!(f, "panicked:\n{}", self.message),
        }
    }
}

impl std::error::Error for GuestPanic {}

/// The host's reading of how an entry ended, owned on its side: what it answered, or the panic it caught at its edge, from [`CallOutcome::into_result`].
pub type GuestResult<T> = Result<T, GuestPanic>;

static NEXT_CALL: AtomicU64 = AtomicU64::new(1);

/// Panics the hook has seen inside a [`guest_call`], by the call they happened in. A static of this copy of std, so one per guest; deliberately not a `thread_local`, whose `Drop` would register a destructor that keeps an unloaded guest mapped until its thread exits.
static RECORDED: Mutex<Vec<(u64, GuestPanic)>> = Mutex::new(Vec::new());

reactive_local::dropless_thread_local! {
    static CURRENT_CALL: Cell<u64> = const { Cell::new(0) };
}

fn recorded() -> MutexGuard<'static, Vec<(u64, GuestPanic)>> {
    RECORDED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs one entry's body, catching a panic before it can leave the guest: the catch under [`guest_entry`].
pub(crate) fn guest_call<T>(call: impl FnOnce() -> T) -> GuestResult<T> {
    record_panics();
    let id = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let outer = CURRENT_CALL.replace(id);
    let result = catch_unwind(AssertUnwindSafe(call));
    CURRENT_CALL.set(outer);
    let seen = {
        let mut recorded = recorded();
        recorded
            .iter()
            .position(|(call, _)| *call == id)
            .map(|index| recorded.swap_remove(index).1)
    };
    result.map_err(|payload| {
        seen.unwrap_or_else(|| GuestPanic {
            message: payload_message(payload.as_ref()),
            location: None,
        })
    })
}

/// Chains a hook in front of whatever this std copy had, so a caught panic keeps its location; the hook it chains to is still the one that prints it, once.
fn record_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let call = CURRENT_CALL.try_with(Cell::get).unwrap_or(0);
            if call != 0 {
                let panic = GuestPanic {
                    message: payload_message(info.payload()),
                    location: info.location().map(ToString::to_string),
                };
                recorded().push((call, panic));
            }
            previous(info);
        }));
    });
}

fn payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Box<dyn Any>".to_owned()
    }
}

/// The host's record of one loaded guest: whether it has panicked, what the panic said, and whether anybody has been told.
///
/// A guest that panicked is never called again. The unwind left whatever it was in the middle of half done — a borrow released mid-update, a batch never closed — and the next call would run on top of that.
#[derive(Default)]
pub struct Guest {
    panic: OnceCell<GuestPanic>,
    reported: Cell<bool>,
}

impl Guest {
    /// Makes one call into the guest, unless it has already panicked. `None` when it had, or does now.
    pub fn call<T>(&self, call: impl FnOnce() -> GuestResult<T>) -> Option<T> {
        if self.has_panicked() {
            return None;
        }
        match call() {
            Ok(value) => Some(value),
            Err(panic) => {
                tracing::error!("a guest panicked; nothing more of it runs");
                let _ = self.panic.set(panic);
                None
            }
        }
    }

    pub fn has_panicked(&self) -> bool {
        self.panic.get().is_some()
    }

    /// The panic that stopped the guest, for as long as the guest is held.
    pub fn panic(&self) -> Option<&GuestPanic> {
        self.panic.get()
    }

    /// The panic that stopped the guest, the first time it is asked for: what a host polling once a frame shows exactly once.
    pub fn take_panic(&self) -> Option<GuestPanic> {
        let panic = self.panic.get()?;
        (!self.reported.replace(true)).then(|| panic.clone())
    }

    /// Unmaps a guest by dropping `library`: tells it it is `unloading` unless it panicked, has it `quiesce` its threads either way, and keeps `library` for the rest of the process if they could not be joined, since unmapping code a thread may still run is never safe and leaking the mapping only costs memory. `library` is everything those threads may still reach, the backends its services call into with the mapping, so all of it is leaked together.
    pub fn unload<L>(
        &self,
        library: L,
        unloading: impl FnOnce() -> GuestResult<()>,
        quiesce: impl FnOnce() -> GuestResult<()>,
    ) {
        self.call(unloading);
        match quiesce() {
            Ok(()) => drop(library),
            Err(panic) => {
                tracing::error!(
                    "a guest panicked while its threads were joined, so its library stays mapped: {panic}"
                );
                std::mem::forget(library);
            }
        }
    }
}

/// Runs host code the guest called into, so a panic in it stops on this side: unwinding on into the guest's frames would abort the process.
pub fn shield<T>(fallback: T, call: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        tracing::error!("host code panicked while a guest was calling it");
        fallback
    })
}

#[cfg(test)]
#[path = "guest_test.rs"]
mod tests;
