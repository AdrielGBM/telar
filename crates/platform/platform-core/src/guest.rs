//! A dylib that carries its own copy of the runtime, and the boundary between it and the host that loads it: hot reload swapping a whole application in, and an embedded guest compositing into the host's frame.
//!
//! Each side links its own copy of std, so a panic unwinding out of one into the other is a foreign exception there, and the only thing a Rust runtime does with one of those is abort the process. So nothing unwinds across: every export runs its body under [`guest_call`] and hands back a [`GuestResult`], the host makes every call through a [`Guest`] that stops calling once one has panicked, and host code the guest is handed catches its own panics with [`shield`] before they reach a frame of the guest's.

use std::any::Any;
use std::cell::{Cell, OnceCell};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use crate::{AppCtx, RedrawWaker};

/// Opens `path` with the loading policy a guest needs: resolve every symbol now, and keep the guest's symbols **local** so its own copy of the runtime never interposes on the host's.
///
/// Two runtimes in one process is the premise of every model that loads one, and each is only sound while the two stay apart. A global-scope load lets the guest's copy of a `thread_local` accessor answer for the host's, and what that looks like is not a crash: it is a registry the host fills and the guest never reads, silently, for the life of the process.
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
pub fn open(path: &std::path::Path) -> Result<libloading::Library, libloading::Error> {
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
/// A guest dropped afterwards is unmapped by `dlclose`, except that on glibc a thread-local with a destructor keeps its library mapped until the thread exits. No thread-local of the runtime has one, the panic slot behind [`guest_call`] included, and a guest told it was [`loaded`] registers none of its own either.
#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
pub fn open_unique(
    path: &std::path::Path,
) -> Result<libloading::Library, Box<dyn std::error::Error>> {
    let unique = unique_copy_path(path);
    std::fs::copy(path, &unique)?;
    let library = open(&unique);
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

/// A fingerprint of the layout of every type that crosses the boundary, and of the features that change one, taken by each side from the same list of definitions.
///
/// A version number only says the exports were last edited together. The host and the guest are two builds, and a type that crosses by value is laid out by whichever features and dependencies each was built with: a host and a guest that differ there would read each other's memory as the wrong shape instead of failing. Comparing fingerprints before anything else is called turns that into a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryLayout(pub u64);

impl BoundaryLayout {
    /// Hashes each type's name, size and alignment, then each named feature and whether it is on.
    pub const fn of(types: &[(&str, usize, usize)], features: &[(&str, bool)]) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325;
        let mut index = 0;
        while index < types.len() {
            let (name, size, align) = types[index];
            hash = hash_bytes(hash, name.as_bytes());
            hash = hash_bytes(hash, &(size as u64).to_le_bytes());
            hash = hash_bytes(hash, &(align as u64).to_le_bytes());
            index += 1;
        }
        index = 0;
        while index < features.len() {
            let (name, on) = features[index];
            hash = hash_bytes(hash, name.as_bytes());
            hash = hash_bytes(hash, &[on as u8]);
            index += 1;
        }
        Self(hash)
    }

    /// Refuses a guest whose fingerprint is not this one.
    pub fn accept(self, guest: Self) -> Result<(), LayoutMismatch> {
        if self == guest {
            Ok(())
        } else {
            Err(LayoutMismatch { host: self, guest })
        }
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

/// Fingerprints [`size_of`](std::mem::size_of) and [`align_of`](std::mem::align_of) of each listed type and the listed `feature => on` pairs, as a const [`BoundaryLayout`].
#[macro_export]
macro_rules! boundary_layout {
    ($($ty:ty),+ $(,)?; $($feature:literal => $on:expr),* $(,)?) => {
        $crate::guest::BoundaryLayout::of(
            &[$((stringify!($ty), ::std::mem::size_of::<$ty>(), ::std::mem::align_of::<$ty>())),+],
            &[$(($feature, $on)),*],
        )
    };
}

/// A guest laid out what crosses the boundary differently from its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutMismatch {
    pub host: BoundaryLayout,
    pub guest: BoundaryLayout,
}

impl fmt::Display for LayoutMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the host and the guest lay out what crosses between them differently (host {:016x}, guest {:016x}): they were built with different features or dependencies, so restart the session, or rebuild both with matching features",
            self.host.0, self.guest.0
        )
    }
}

impl std::error::Error for LayoutMismatch {}

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

/// What every export returns in place of unwinding.
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

/// Runs one export's body, catching a panic before it can leave the guest.
pub fn guest_call<T>(call: impl FnOnce() -> T) -> GuestResult<T> {
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

/// What a host calls first on a guest it loaded from a library and will unload: from then on this copy of the runtime registers no thread-exit destructor, which on glibc would keep the library mapped after it is closed, and leaves what it would have freed at thread exit to [`unloading`].
pub fn loaded() -> GuestResult<()> {
    guest_call(reactive_local::thread_exit::leave_to_host)
}

/// What a host calls last on a guest, right before it unloads the library: frees what this copy kept for the calling thread, its reactive runtime and every registry among it, which would otherwise wait for a thread exit that comes after the library is gone, and what its statics hold, which nothing would ever free. Its worker threads are joined first, so no code of the library runs once it is unmapped.
pub fn unloading() -> GuestResult<()> {
    guest_call(reactive_local::thread_exit::release_guest)
}

/// What a host calls on every guest it loaded from a library, after [`unloading`] and right before it unmaps it, a guest that panicked included: joins the threads that run the library's code, touching nothing a panic may have left half done. See [`Guest::unload`].
pub fn quiesce() -> GuestResult<()> {
    guest_call(reactive_local::thread_exit::quiesce)
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

    /// Unmaps a guest by dropping `library`: tells it it is `unloading` unless it panicked, has it `quiesce` its threads either way, and keeps `library` mapped for the rest of the process if they could not be joined, since unmapping code a thread may still run is never safe and leaking the mapping only costs memory.
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

/// A waker to hand a guest: waking the host's loop is host code, which the guest may call from any of its threads.
pub fn shielded_waker(waker: RedrawWaker) -> RedrawWaker {
    RedrawWaker::new(move || shield((), || waker.wake()))
}

/// Runs `call` with a frame context for the guest: `ctx`'s handles, a waker that keeps a panic on this side, and a redraw the guest requests carried back to `ctx`.
pub fn with_guest_ctx<T>(ctx: &mut AppCtx<'_>, call: impl FnOnce(&mut AppCtx<'_>) -> T) -> T {
    let waker = ctx.redraw_waker().map(shielded_waker);
    let mut redraw_requested = false;
    let mut guest_ctx = AppCtx::new(
        &mut redraw_requested,
        waker.as_ref(),
        ctx.raw_window_handle(),
        ctx.raw_display_handle(),
    );
    let out = call(&mut guest_ctx);
    if redraw_requested {
        ctx.request_redraw();
    }
    out
}

#[cfg(test)]
#[path = "guest_test.rs"]
mod tests;
