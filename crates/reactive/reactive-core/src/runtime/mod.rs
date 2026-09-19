//! The runtime itself: the arenas holding signals, effects and owners, and the thread-local cell they live in.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::panic::Location;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SlotMap;

mod effects;
mod flush;
mod owner;
mod signals;
mod surface;

pub(crate) use effects::{
    current_observer, force_run_effect, is_alive, register_effect, register_pure_effect,
    run_effect, schedule, untracked,
};
pub use flush::{after_settle, batch, begin_batch, end_batch, reset_runtime, set_flush_notify};
pub(crate) use owner::is_surface_disposed;
pub use owner::{
    OwnerGuard, OwnerId, PanicCatch, PanicPayload, context_provided_here, current_owner,
    dispose_owner, dispose_surface, dispose_surface_owners, find_context, in_surface_world,
    live_effect_count, live_owner_count, live_signal_count, on_cleanup, owner_scope, owner_within,
    provide_context, set_panic_catch, with_context, with_owner,
};
pub(crate) use signals::{
    claim_transaction, create_signal_storage, notify_signal, release_transaction, set_signal_value,
    signal_is_alive, signal_version, track_signal, try_update_signal_value, try_with_signal_value,
    update_signal_value, with_signal_value,
};
pub use surface::{
    SurfaceEnterGuard, SurfaceHandle, current_surface, set_current_surface, set_surface_enter_hook,
};

// Versioned, not raw arena indices: a freed slot is handed straight back, so under a plain index a handle outliving its signal addressed whatever moved in — same type, same index, wrong signal, no panic.
slotmap::new_key_type! {
    pub(crate) struct EffectId;
    pub(crate) struct SignalId;
}

pub(crate) struct EffectEntry {
    pub(crate) callback: Box<dyn Fn()>,
    // The surface active when this effect was registered; the flush re-enters it before running the callback, so a cross-surface write resolves the effect against its own surface's world.
    pub(crate) surface: SurfaceHandle,
    /// The owner active at registration, re-entered for each run so a re-run creates and registers under the scope that built it rather than under whatever the flush happens to be inside.
    pub(crate) owner: Option<OwnerId>,
    pub(crate) is_pure: bool,
    pub(crate) last_run_epoch: u64,
    pub(crate) sources: Vec<SignalId>,
    pub(crate) source_slots: Vec<usize>,
    pub(crate) source_versions: Vec<u64>,
    // Topological height; 0 is a leaf with no tracked sources.
    pub(crate) height: u32,
    // Set when a memo schedules this effect. Memo dependencies are invisible to `sources`, so `run_effect`'s version check must be bypassed once or an effect also tracking an unchanged signal would never run.
    pub(crate) memo_dirty: bool,
}

pub(crate) struct SignalStorage {
    pub(crate) value: Box<dyn std::any::Any>,
    pub(crate) version: u64,
    pub(crate) subscribers: Vec<EffectId>,
    // For each subscriber, the index in that effect's `source_slots` recording this signal.
    pub(crate) observer_slots: Vec<usize>,
}

pub(crate) struct Runtime {
    pub(crate) observer_stack: Vec<EffectId>,
    /// How many effect runs are on the call stack. Not the observer stack's length: `untracked` hides that stack while the run that called it is still executing.
    pub(crate) running_effects: usize,
    pub(crate) owners: SlotMap<OwnerId, owner::OwnerEntry>,
    pub(crate) owner_stack: Vec<OwnerId>,
    /// The owner everything created outside every scope belongs to, one per surface, minted on demand.
    pub(crate) roots: FxHashMap<SurfaceHandle, OwnerId>,
    /// The owner a surface's own worlds belong to, disposed after everything else on the surface: see [`in_surface_world`](owner::in_surface_world).
    pub(crate) worlds: FxHashMap<SurfaceHandle, OwnerId>,
    /// Every surface [`dispose_surface`](owner::dispose_surface) has torn down, so a callback queued against one before it settles (see [`after_settle`](flush::after_settle)) can tell its surface is gone rather than running under whatever world happens to be active. Surface ids are never reused, so this only grows.
    pub(crate) disposed_surfaces: FxHashSet<SurfaceHandle>,
    pub(crate) effects: SlotMap<EffectId, EffectEntry>,
    pub(crate) signals: SlotMap<SignalId, SignalStorage>,
    pub(crate) batch_depth: usize,
    pub(crate) pending: Vec<EffectId>,
    pub(crate) memo_pending: BinaryHeap<(Reverse<u32>, EffectId)>,
    pub(crate) pending_set: FxHashSet<EffectId>,
    /// The signals a [`crate::Transaction`] is open on, so a second one on the same signal is refused.
    pub(crate) transactions: FxHashSet<SignalId>,
    // Reused to copy a signal's subscribers out before scheduling, instead of allocating per write.
    subscriber_scratch: Vec<EffectId>,
    flush_callbacks: Vec<(u64, Rc<dyn Fn()>)>,
    /// Work that waits for every write so far to reach its effects: see [`after_settle`](flush::after_settle).
    pub(crate) settle_queue: Vec<(SurfaceHandle, Box<dyn FnOnce()>)>,
    next_flush_callback_id: u64,
    /// How many teardowns are in flight. Non-zero means the arenas are mid-surgery — see `owner::Disposing` — and `flush` stays out until it is back to zero.
    pub(crate) disposing: usize,
    pub(crate) flushing: bool,
    flush_epoch: u64,
    /// The (effect, signal) pairs already reported as waking themselves, so the warning is said once rather than on every pass of a loop that has no other end.
    #[cfg(debug_assertions)]
    pub(crate) self_waking: FxHashSet<(EffectId, SignalId)>,
}

impl Runtime {
    fn new() -> Self {
        Runtime {
            observer_stack: Vec::new(),
            running_effects: 0,
            owners: SlotMap::with_key(),
            owner_stack: Vec::new(),
            roots: FxHashMap::default(),
            worlds: FxHashMap::default(),
            disposed_surfaces: FxHashSet::default(),
            effects: SlotMap::with_key(),
            signals: SlotMap::with_key(),
            batch_depth: 0,
            pending: Vec::new(),
            memo_pending: BinaryHeap::new(),
            pending_set: FxHashSet::default(),
            transactions: FxHashSet::default(),
            subscriber_scratch: Vec::new(),
            flush_callbacks: Vec::new(),
            settle_queue: Vec::new(),
            next_flush_callback_id: 0,
            disposing: 0,
            flushing: false,
            flush_epoch: 0,
            #[cfg(debug_assertions)]
            self_waking: FxHashSet::default(),
        }
    }
}

// The runtime is a heap Box behind a raw pointer. `*mut T` has no `Drop`, so neither does this, and `thread_local!` registers no TLS destructor — which is what makes dlclosing a hot-reload dylib safe.
struct RuntimeCell {
    ptr: Cell<*mut RefCell<Runtime>>,
    /// Where the last borrow to succeed was taken, so a collision can name what it collided with rather than only itself. See [`crate::reentry`]. `Option<&'static Location>` has no `Drop`, so this keeps the cell free of a TLS destructor.
    last_borrow: Cell<Option<&'static Location<'static>>>,
}

impl RuntimeCell {
    #[track_caller]
    fn borrow_mut(&self) -> RefMut<'_, Runtime> {
        self.enter(unsafe { (*self.ptr.get()).try_borrow_mut() }.ok())
    }

    #[track_caller]
    fn borrow(&self) -> Ref<'_, Runtime> {
        self.enter(unsafe { (*self.ptr.get()).try_borrow() }.ok())
    }

    /// Swaps in a fresh runtime and hands back the old pointer for the caller to drop. The recorded borrow site goes with it: it names a call into a runtime that no longer exists.
    fn take_ptr(&self) -> *mut RefCell<Runtime> {
        self.last_borrow.set(None);
        self.ptr
            .replace(Box::into_raw(Box::new(RefCell::new(Runtime::new()))))
    }

    #[track_caller]
    fn enter<G>(&self, borrowed: Option<G>) -> G {
        let here = Location::caller();
        match borrowed {
            Some(guard) => {
                self.last_borrow.set(Some(here));
                guard
            }
            None => crate::reentry::borrow_collision("RUNTIME", self.last_borrow.get(), here),
        }
    }
}

thread_local! {
    static RUNTIME: RuntimeCell = RuntimeCell {
        ptr: Cell::new(Box::into_raw(Box::new(RefCell::new(Runtime::new())))),
        last_borrow: const { Cell::new(None) },
    };
}

/// Holds the installed flush callback; dropping it removes the callback.
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

#[cfg(test)]
#[path = "runtime_test.rs"]
mod tests;
