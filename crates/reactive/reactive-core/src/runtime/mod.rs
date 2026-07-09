use std::cell::{Cell, Ref, RefCell, RefMut};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::rc::Rc;

use rustc_hash::FxHashSet;

mod effects;
mod flush;
mod signals;

pub(crate) use effects::{
    current_observer, deregister_effect, is_alive, register_effect, register_pure_effect,
    run_effect, schedule,
};
pub use flush::{batch, begin_batch, end_batch, reset_runtime, set_flush_notify};
pub(crate) use signals::{
    clone_signal, create_signal_storage, drop_signal, notify_signal, set_signal_value,
    track_signal, update_signal_value, with_signal_value,
};

pub(crate) type EffectId = usize;
pub(crate) type SignalId = usize;

pub(crate) struct EffectEntry {
    pub(crate) callback: Box<dyn Fn()>,
    pub(crate) is_pure: bool,
    pub(crate) last_run_epoch: u64,
    pub(crate) sources: Vec<SignalId>,
    pub(crate) source_slots: Vec<usize>,
    pub(crate) source_versions: Vec<u64>,
    // Topological height; 0 = leaf (no tracked sources).
    pub(crate) height: u32,
    // Set when a memo schedules this effect. Memo dependencies are invisible to `sources` (they live in MemoInner.subscribers, not in a SignalStorage), so run_effect's version check must be bypassed once or an effect that also tracks an unchanged signal would be skipped forever.
    pub(crate) memo_dirty: bool,
}

pub(crate) struct SignalStorage {
    pub(crate) value: Box<dyn std::any::Any>,
    pub(crate) version: u64,
    pub(crate) subscribers: Vec<EffectId>,
    // For each subscriber: the index in that effect's `source_slots` vec that records this signal.
    pub(crate) observer_slots: Vec<usize>,
    // Handle reference count; the slab slot is freed when it reaches zero.
    pub(crate) ref_count: usize,
}

pub(crate) struct Runtime {
    pub(crate) observer_stack: Vec<EffectId>,
    pub(crate) effects: slab::Slab<EffectEntry>,
    pub(crate) signals: slab::Slab<SignalStorage>,
    pub(crate) batch_depth: usize,
    pub(crate) pending: Vec<EffectId>,
    pub(crate) memo_pending: BinaryHeap<(Reverse<u32>, EffectId)>,
    pub(crate) pending_set: FxHashSet<EffectId>,
    // Reused by notify_signal to copy a signal's subscribers out before scheduling, instead of allocating a fresh Vec per write.
    subscriber_scratch: Vec<EffectId>,
    flush_callbacks: Vec<(u64, Rc<dyn Fn()>)>,
    next_flush_callback_id: u64,
    pub(crate) flushing: bool,
    flush_epoch: u64,
}

impl Runtime {
    fn new() -> Self {
        Runtime {
            observer_stack: Vec::new(),
            effects: slab::Slab::new(),
            signals: slab::Slab::new(),
            batch_depth: 0,
            pending: Vec::new(),
            memo_pending: BinaryHeap::new(),
            pending_set: FxHashSet::default(),
            subscriber_scratch: Vec::new(),
            flush_callbacks: Vec::new(),
            next_flush_callback_id: 0,
            flushing: false,
            flush_epoch: 0,
        }
    }
}

// RuntimeCell stores the runtime as a heap-allocated Box behind a raw pointer. Because *mut T has no Drop, Cell<*mut T> has no Drop, and this struct has no Drop either. That means thread_local! won't register a TLS destructor for RUNTIME — so dlclosing the dylib during hot reload no longer causes "double free or corruption" when the thread exits.
struct RuntimeCell(Cell<*mut RefCell<Runtime>>);

impl RuntimeCell {
    fn borrow_mut(&self) -> RefMut<'_, Runtime> {
        unsafe { (*self.0.get()).borrow_mut() }
    }
    fn borrow(&self) -> Ref<'_, Runtime> {
        unsafe { (*self.0.get()).borrow() }
    }
}

thread_local! {
    static RUNTIME: RuntimeCell = RuntimeCell(Cell::new(
        Box::into_raw(Box::new(RefCell::new(Runtime::new())))
    ));
}

/// An owned, inactive reactive runtime for a single surface/tree, isolated from the thread's default runtime
/// and from every other surface's runtime. A multi-surface backend that multiplexes several surfaces on **one
/// thread** activates a surface's runtime with [`SurfaceRuntime::enter`] around that surface's event/render
/// callbacks, so its signals, effects, and memos resolve against this runtime rather than the shared one —
/// giving each surface an isolated reactive world without a separate OS thread. (A backend that runs each
/// surface on its own thread gets the same isolation for free, since the runtime is thread-local.)
///
/// Not `Send`: it is a handle into thread-local runtime storage and must stay on the thread that created it.
pub struct SurfaceRuntime {
    ptr: *mut RefCell<Runtime>,
}

impl SurfaceRuntime {
    /// Allocates a fresh, empty runtime. It is inactive until [`enter`](Self::enter) is called.
    pub fn new() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(RefCell::new(Runtime::new()))),
        }
    }

    /// Makes this the current thread's active runtime for as long as the returned [`RuntimeGuard`] lives; the
    /// previously-active runtime is restored when the guard drops. Nest by keeping guards in scope; drop them
    /// in reverse order (a plain `let _guard = rt.enter();` does this naturally).
    pub fn enter(&self) -> RuntimeGuard {
        let prev = RUNTIME.with(|cell| {
            let prev = cell.0.get();
            cell.0.set(self.ptr);
            prev
        });
        RuntimeGuard { prev }
    }
}

impl Default for SurfaceRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SurfaceRuntime {
    fn drop(&mut self) {
        // A matching guard restores the previous runtime before this drops, so `ptr` is never the active
        // pointer here — freeing it cannot pull the rug out from an in-progress access.
        if !self.ptr.is_null() {
            unsafe { drop(Box::from_raw(self.ptr)) };
        }
    }
}

/// Restores the runtime that was active before the matching [`SurfaceRuntime::enter`] when dropped.
#[must_use = "the runtime is only active while this guard is alive"]
pub struct RuntimeGuard {
    prev: *mut RefCell<Runtime>,
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        RUNTIME.with(|cell| cell.0.set(self.prev));
    }
}

pub struct FlushNotifyHandle {
    id: u64,
}

impl Drop for FlushNotifyHandle {
    fn drop(&mut self) {
        deregister_flush_notify(self.id);
    }
}

fn deregister_flush_notify(id: u64) {
    RUNTIME.with(|rt| {
        rt.borrow_mut()
            .flush_callbacks
            .retain(|(entry_id, _)| *entry_id != id);
    });
}
