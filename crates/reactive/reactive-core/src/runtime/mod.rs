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
    current_observer, is_alive, register_effect, register_pure_effect, run_effect, schedule,
};
pub use flush::{batch, begin_batch, end_batch, reset_runtime, set_flush_notify};
pub use owner::{
    OwnerGuard, OwnerId, context_provided_here, current_owner, dispose_owner,
    dispose_surface_owners, live_effect_count, live_signal_count, on_cleanup, owner_scope,
    provide_context, with_context, with_owner,
};
pub(crate) use signals::{
    create_signal_storage, notify_signal, set_signal_value, track_signal, update_signal_value,
    with_signal_value,
};
pub use surface::{
    SurfaceEnterGuard, SurfaceHandle, current_surface, set_current_surface, set_surface_enter_hook,
};

// Versioned, not raw arena indices: a freed slot is handed straight back, so under a plain index a handle outliving its signal addressed whatever moved in — and `with_signal_value` only noticed when the new value had a different type. Same type, same index, wrong signal, no panic. The version is what turns that read into a miss.
slotmap::new_key_type! {
    pub(crate) struct EffectId;
    pub(crate) struct SignalId;
}

pub(crate) struct EffectEntry {
    pub(crate) callback: Box<dyn Fn()>,
    // The surface active when this effect was registered; the flush re-enters it before running the
    // callback so a cross-surface signal write resolves the effect against its own surface's world.
    pub(crate) surface: SurfaceHandle,
    /// The owner active at registration, re-entered for each run so a re-run creates and registers under the
    /// scope that built it rather than under whatever the flush happens to be inside.
    pub(crate) owner: Option<OwnerId>,
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
}

pub(crate) struct Runtime {
    pub(crate) observer_stack: Vec<EffectId>,
    pub(crate) owners: SlotMap<OwnerId, owner::OwnerEntry>,
    pub(crate) owner_stack: Vec<OwnerId>,
    /// The owner everything created outside every scope belongs to, one per surface, minted on demand.
    pub(crate) roots: FxHashMap<SurfaceHandle, OwnerId>,
    pub(crate) effects: SlotMap<EffectId, EffectEntry>,
    pub(crate) signals: SlotMap<SignalId, SignalStorage>,
    pub(crate) batch_depth: usize,
    pub(crate) pending: Vec<EffectId>,
    pub(crate) memo_pending: BinaryHeap<(Reverse<u32>, EffectId)>,
    pub(crate) pending_set: FxHashSet<EffectId>,
    // Reused by notify_signal to copy a signal's subscribers out before scheduling, instead of allocating a fresh Vec per write.
    subscriber_scratch: Vec<EffectId>,
    flush_callbacks: Vec<(u64, Rc<dyn Fn()>)>,
    next_flush_callback_id: u64,
    /// How many teardowns are in flight. Non-zero means the arenas are mid-surgery — see `owner::Disposing` — and `flush` stays out until it is back to zero.
    pub(crate) disposing: usize,
    pub(crate) flushing: bool,
    flush_epoch: u64,
}

impl Runtime {
    fn new() -> Self {
        Runtime {
            observer_stack: Vec::new(),
            owners: SlotMap::with_key(),
            owner_stack: Vec::new(),
            roots: FxHashMap::default(),
            effects: SlotMap::with_key(),
            signals: SlotMap::with_key(),
            batch_depth: 0,
            pending: Vec::new(),
            memo_pending: BinaryHeap::new(),
            pending_set: FxHashSet::default(),
            subscriber_scratch: Vec::new(),
            flush_callbacks: Vec::new(),
            next_flush_callback_id: 0,
            disposing: 0,
            flushing: false,
            flush_epoch: 0,
        }
    }
}

// RuntimeCell stores the runtime as a heap-allocated Box behind a raw pointer. Because *mut T has no Drop, Cell<*mut T> has no Drop, and this struct has no Drop either. That means thread_local! won't register a TLS destructor for RUNTIME — so dlclosing the dylib during hot reload no longer causes "double free or corruption" when the thread exits.
struct RuntimeCell {
    ptr: Cell<*mut RefCell<Runtime>>,
    /// Where the last borrow to succeed was taken, so a collision can name what it collided with rather than
    /// only itself. See [`crate::reentry`]. `Option<&'static Location>` has no `Drop`, so this keeps the cell
    /// free of a TLS destructor.
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

    /// Swaps in a fresh runtime and hands back the old pointer for the caller to drop. The recorded borrow
    /// site goes with it: it names a call into a runtime that no longer exists.
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
mod tests {
    use super::*;

    /// What a reentrant borrow used to say was `already borrowed: BorrowMutError`, over a backtrace of the
    /// runtime's own frames: the call that collided is in there somewhere, the call it collided *with* never
    /// is — and that second one is the whole of the diagnosis, because it is the operation still on the stack
    /// that came back round.
    #[test]
    fn a_reentrant_runtime_borrow_names_both_call_sites() {
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            RUNTIME.with(|rt| {
                let _held = rt.borrow_mut();
                let _collides = rt.borrow_mut();
            });
        }));
        std::panic::set_hook(quiet);

        let payload = outcome.expect_err("the second borrow cannot succeed");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        assert!(
            message.contains("`RUNTIME` is already borrowed"),
            "{message}"
        );
        assert_eq!(
            message.matches(file!()).count(),
            2,
            "both sites are named, and both are in this file:\n{message}"
        );
    }
}
