//! The owner tree — what disposes reactive state, once anything does.
//!
//! Every signal, memo and effect belongs to a node in a tree, recorded at creation from whatever owner is active, the same way [`EffectEntry::surface`](super::EffectEntry) records the surface. Disposing an owner walks its children first, then frees what it holds itself. A view mints one per component instance, per reactive `if` branch and per list row.
//!
//! # How this composes with `surface_local!`
//!
//! **A surface is the outer world; an owner is a node in the inner tree.** They are not competing scoping mechanisms and must not be read as one.
//!
//! A surface owns a set of swappable thread-local worlds — its layout tree, overlays, focus, input region, window-command queue (`ui-core/src/surface_context.rs`). Those keep their job exactly as it is. An owner owns *reactive state*: the entries in this runtime's arenas. An owner belongs to exactly one surface, the one active when it was created, and disposing a surface disposes the owner roots stamped with it — never the reverse. Owners nest inside a surface; a surface never nests inside an owner.
//!
//! The reason to write it down: both answer "who cleans this up", so the temptation is to fold one into the other. They clean up different things, on different schedules — a surface lives for a window, an owner for a component instance, an `if` branch, or one row of a list.

use std::any::{Any, TypeId};
use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::flush::{batch, flush};
use super::surface::current_surface;
use super::{EffectId, RUNTIME, Runtime, SignalId, SurfaceHandle};
use crate::runtime::effects::deregister_effect;

slotmap::new_key_type! {
    pub struct OwnerId;
}

pub(crate) struct OwnerEntry {
    parent: Option<OwnerId>,
    children: Vec<OwnerId>,
    signals: Vec<SignalId>,
    effects: Vec<EffectId>,
    /// What this owner has to undo elsewhere. The escape hatch for state an owner's lifetime governs but this crate cannot name — a cascade declaration is keyed by a layout `NodeId`, which lives three crates up.
    cleanups: Vec<Box<dyn FnOnce()>>,
    /// What this owner tells everything below it, keyed by type. Empty for almost every owner: only a scope that provides something ever reaches the allocator for this.
    context: FxHashMap<TypeId, Rc<dyn Any>>,
    surface: SurfaceHandle,
}

impl OwnerEntry {
    // A 10k-row list is 10k of these, so an owner holding nothing must not reach the allocator.
    fn new(parent: Option<OwnerId>, surface: SurfaceHandle) -> Self {
        OwnerEntry {
            parent,
            children: Vec::new(),
            signals: Vec::new(),
            effects: Vec::new(),
            cleanups: Vec::new(),
            context: FxHashMap::default(),
            surface,
        }
    }
}

/// The owner a signal or effect created right now would belong to, or `None` outside every owner scope.
///
/// `None` does not mean unowned: creation falls back to the active surface's root, which `owning_id` mints on demand. This answers the narrower question of whether a scope is open.
pub fn current_owner() -> Option<OwnerId> {
    RUNTIME.with(|rt| rt.borrow().owner_stack.last().copied())
}

/// Opens a fresh owner as a child of the active one and makes it current until the guard drops.
///
/// **The active one is whoever would own a context**, which with an empty stack is the surface's root — the same rule [`provide_context`] and [`with_context`] read by. Parenting off the bare stack instead made a scope opened at the top of a surface build an orphan: the builder had written its context on the surface root, and the walk up from the orphan never reached it. Every `.rsx` component opens a scope like this, so a panel asking which module it was built for read an empty string and drew the fallback.
pub fn owner_scope() -> OwnerGuard {
    let surface = current_surface();
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let parent = owning_id(&mut rt);
        let id = rt.owners.insert(OwnerEntry::new(parent, surface));
        if let Some(parent) = parent
            && let Some(entry) = rt.owners.get_mut(parent)
        {
            entry.children.push(id);
        }
        let depth = rt.owner_stack.len();
        rt.owner_stack.push(id);
        OwnerGuard { id, depth }
    })
}

/// Restores the owner scope that was active before [`owner_scope`].
///
/// Truncates to the depth recorded on entry rather than popping one, so an unwind through a build that left inner scopes open still lands the stack where it started. An unbalanced stack is the worst kind of bug to leave behind: every later creation goes to the wrong owner, and being a lifetime error it surfaces nowhere near the panic that caused it. `batch_depth` meets this standard already.
#[must_use = "the owner scope is only active while this guard is alive"]
pub struct OwnerGuard {
    id: OwnerId,
    depth: usize,
}

impl OwnerGuard {
    pub fn id(&self) -> OwnerId {
        self.id
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        RUNTIME.with(|rt| rt.borrow_mut().owner_stack.truncate(self.depth));
    }
}

// Both take the runtime rather than reaching for it, so recording an owner costs nothing beyond a push on the borrow the creation already holds.
pub(crate) fn attach_signal(rt: &mut Runtime, id: SignalId) {
    if let Some(entry) = owning(rt) {
        entry.signals.push(id);
    }
}

pub(crate) fn attach_effect(rt: &mut Runtime, id: EffectId) {
    if let Some(entry) = owning(rt) {
        entry.effects.push(id);
    }
}

/// The owner a creation right now belongs to, minting the active surface's root if the stack is empty.
///
/// **Every surface has a root, including the ambient one** — `SurfaceHandle::NONE`, which is the whole world a single-window app ever has. Without it, everything a top-level view creates belongs to nobody: harmless while the handle refcount was still what freed things, and a plain leak once it is not. It is also what a declaration made outside every scope needs, since Phase 4 made withdrawal an owner's job.
///
/// The cost is the trade this whole design makes, and it lands here: a signal created outside every scope now lives until its surface does, where the refcount would have freed it at the last handle. In a UI that is almost always what you wanted; it is still a change.
///
/// `None` means one thing only: [`detached`](reactive_local::detached). A `surface_local!` world initialises on first access, and the first access is somebody's build — attributing a surface's own state to whatever row happened to touch it would free it when that row went away.
fn owning(rt: &mut Runtime) -> Option<&mut OwnerEntry> {
    let id = owning_id(rt)?;
    rt.owners.get_mut(id)
}

pub(crate) fn owning_id(rt: &mut Runtime) -> Option<OwnerId> {
    if reactive_local::is_detached() {
        return None;
    }
    if let Some(owner) = rt.owner_stack.last().copied() {
        return Some(owner);
    }
    let surface = current_surface();
    if let Some(root) = rt.roots.get(&surface).copied()
        && rt.owners.contains_key(root)
    {
        return Some(root);
    }
    let root = rt.owners.insert(OwnerEntry::new(None, surface));
    rt.roots.insert(surface, root);
    Some(root)
}

/// Runs `f` under `owner`, for code that executes long after the build that made it.
///
/// The counterpart to the surface re-entry in `run_effect`, and needed for the same reason one layer in. An event handler is a plain closure: by the time it is called the owner stack is empty, so anything it asks for ambiently would resolve against the surface root rather than against the component it belongs to. Mints nothing — the owner already exists, and the handler must land *in* it rather than beside it.
pub fn with_owner<R>(owner: Option<OwnerId>, f: impl FnOnce() -> R) -> R {
    let depth = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let depth = rt.owner_stack.len();
        if let Some(owner) = owner
            && rt.owners.contains_key(owner)
        {
            rt.owner_stack.push(owner);
        }
        depth
    });
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            RUNTIME.with(|rt| rt.borrow_mut().owner_stack.truncate(self.0));
        }
    }
    let _restore = Restore(depth);
    f()
}

/// Whether the *current* owner provided a `T` itself, as opposed to inheriting one from above.
///
/// What tells an owner repeating itself apart from one shadowing its parent: the first is a mistake, the second is what nesting is for.
pub fn context_provided_here<T: 'static>() -> bool {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let Some(id) = owning_id(&mut rt) else {
            return false;
        };
        rt.owners
            .get(id)
            .is_some_and(|entry| entry.context.contains_key(&TypeId::of::<T>()))
    })
}

/// Puts `value` in the current owner's context, replacing whatever this owner had of that type.
///
/// Replacing rather than refusing, because a rebuild is the normal case: the same owner builds its content again and says the same things about it. The old spelling worked around a scope that could not be provided to twice by writing through an `Rc<RefCell<T>>` slot, which is a mutable cell standing in for a scope that ends.
pub fn provide_context<T: 'static>(value: T) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(entry) = owning(&mut rt) {
            entry.context.insert(TypeId::of::<T>(), Rc::new(value));
        }
    });
}

/// Reads the nearest value of type `T` at or above the current owner.
///
/// A walk rather than a copy. The scope stack this replaces merged every parent entry into each new scope on entry, so opening one cost an `Rc` clone per inherited service; a walk makes entry free and pays on the read instead, which happens at user speed inside a handler rather than per compound-component build.
pub fn with_context<T: 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    // Cloned out before the borrow is released, because `f` is the caller's and may read a signal, which would re-enter the runtime and abort.
    let found: Rc<dyn Any> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let mut at = owning_id(&mut rt);
        let wanted = TypeId::of::<T>();
        while let Some(id) = at {
            let entry = rt.owners.get(id)?;
            if let Some(value) = entry.context.get(&wanted) {
                return Some(Rc::clone(value));
            }
            at = entry.parent;
        }
        None
    })?;
    found.downcast_ref::<T>().map(f)
}

/// Registers work the current owner runs when it is disposed.
///
/// For state whose lifetime is an owner's but whose *name* is somebody else's — the cascade keys its declarations by layout `NodeId`, which this crate sits three below and cannot mention. A closure crosses that where a field could not.
///
/// Outside every owner the closure is dropped unrun, which is the honest answer: nothing will ever dispose it, so pretending otherwise would only move the leak.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(entry) = owning(&mut rt) {
            entry.cleanups.push(Box::new(f));
        }
    });
}

/// Raised for as long as a teardown is mid-flight, so nothing flushes against a tree that is half taken down.
///
/// The window is the whole of [`dispose_owner`]: [`uproot`] has already removed the owner entries, the effects are still registered and still subscribed, and the signals come out last. An effect that runs in there reads a tree that no longer describes itself — and the read that finds a freed storage panics, in the middle of a window closing, where nothing can act on it.
///
/// Counted rather than a flag, because the nesting is real: a cleanup may dispose another owner, and [`dispose_surface_owners`] takes down a whole set. Only the outermost one is finished when it says it is.
struct Disposing;

impl Disposing {
    fn enter() -> Self {
        RUNTIME.with(|rt| rt.borrow_mut().disposing += 1);
        Disposing
    }
}

impl Drop for Disposing {
    fn drop(&mut self) {
        RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            rt.disposing = rt.disposing.saturating_sub(1);
        });
    }
}

/// Runs what a teardown's cleanups invalidated, once there is no teardown left to run it over.
///
/// Deferred rather than dropped: a cleanup that writes a signal a *surviving* effect reads still has to reach it, and by here everything the teardown was going to free is freed, so the effects that referenced it are deregistered and the flush skips them. What this cannot rescue is an effect that outlives the owner of a memo it reads — that is a lifetime error in the tree itself, and moving the flush only moves where it surfaces.
fn flush_when_settled() {
    let settled = RUNTIME.with(|rt| {
        let rt = rt.borrow();
        rt.disposing == 0
            && rt.batch_depth == 0
            && !rt.flushing
            && (!rt.pending.is_empty() || !rt.memo_pending.is_empty())
    });
    if settled {
        flush();
    }
}

/// Disposes an owner and everything below it, children first.
pub fn dispose_owner(id: OwnerId) {
    let (cleanups, effects, signals) = uproot(id);

    {
        let _disposing = Disposing::enter();

        // One wave, not one per withdrawal: `undeclare` bumps the cascade's `structure` signal, which every context read subscribes to, so tearing down N nodes outside a batch is N invalidations across the tree.
        batch(|| {
            for cleanup in cleanups {
                cleanup();
            }
        });

        for effect in effects {
            deregister_effect(effect);
        }
        // Taken out under the borrow and dropped after releasing it: a signal whose value owns signal handles re-enters the runtime as that value drops, which under the borrow aborts.
        let removed: Vec<Box<dyn Any>> = RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            signals
                .into_iter()
                .filter_map(|signal| rt.signals.remove(signal))
                .map(|storage| storage.value)
                .collect()
        });
        drop(removed);
    }

    flush_when_settled();
}

/// Disposes every owner root belonging to a surface. What `Surface`'s teardown calls.
pub fn dispose_surface_owners(surface: SurfaceHandle) {
    let roots: Vec<OwnerId> = RUNTIME.with(|rt| {
        rt.borrow()
            .owners
            .iter()
            .filter(|(_, entry)| entry.parent.is_none() && entry.surface == surface)
            .map(|(id, _)| id)
            .collect()
    });
    {
        // Held across the whole set rather than left to each root: the roots of one surface read each other, so a flush landing between two of them runs effects over what the one before it just freed.
        let _disposing = Disposing::enter();
        for root in roots {
            dispose_owner(root);
        }
    }
    flush_when_settled();
}

/// How many signals the runtime is holding.
///
/// The arenas are private, so a lifetime that nothing frees is otherwise invisible — a test can watch a row rebuild ten times and see only that it still renders. Owner-scoped disposal makes the count the evidence: it is flat when disposal runs, and climbs when it does not.
pub fn live_signal_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().signals.len())
}

/// How many effects the runtime is holding. See [`live_signal_count`].
pub fn live_effect_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().effects.len())
}

/// Removes an owner's subtree from the arena and hands back what it held, deepest owner first.
///
/// Iterative rather than recursive: the depth is the component nesting of a real view, and a stack overflow inside disposal would abort rather than unwind.
type Uprooted = (Vec<Box<dyn FnOnce()>>, Vec<EffectId>, Vec<SignalId>);

fn uproot(root: OwnerId) -> Uprooted {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();

        if let Some(parent) = rt.owners.get(root).and_then(|entry| entry.parent)
            && let Some(entry) = rt.owners.get_mut(parent)
        {
            entry.children.retain(|child| *child != root);
        }

        let mut order = Vec::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            let Some(entry) = rt.owners.get(id) else {
                continue;
            };
            pending.extend_from_slice(&entry.children);
            order.push(id);
        }

        let mut cleanups = Vec::new();
        let mut effects = Vec::new();
        let mut signals = Vec::new();
        for id in order.into_iter().rev() {
            if let Some(entry) = rt.owners.remove(id) {
                cleanups.extend(entry.cleanups);
                effects.extend(entry.effects);
                signals.extend(entry.signals);
            }
        }
        (cleanups, effects, signals)
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    use super::*;
    use crate::runtime::signals::create_signal_storage;
    use crate::{effect, memo, signal};

    fn alive(id: SignalId) -> bool {
        RUNTIME.with(|rt| rt.borrow().signals.contains_key(id))
    }

    #[test]
    fn disposing_an_owner_takes_its_children_with_it() {
        let outer = owner_scope();
        let held_by_outer = create_signal_storage(1i32);
        let held_by_inner = {
            let _inner = owner_scope();
            create_signal_storage(2i32)
        };

        let root = outer.id();
        drop(outer);
        dispose_owner(root);

        assert!(!alive(held_by_outer));
        assert!(!alive(held_by_inner), "a child owner is disposed with it");
    }

    #[test]
    fn an_effect_stops_running_when_its_owner_is_disposed() {
        let count = signal(0i32);
        let read = count.read_only();
        let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));

        let scope = owner_scope();
        let seen_c = Rc::clone(&seen);
        effect(move || seen_c.borrow_mut().push(read.get()));
        let owner = scope.id();
        drop(scope);

        count.set(1);
        assert_eq!(*seen.borrow(), vec![0, 1]);

        dispose_owner(owner);
        count.set(2);
        assert_eq!(*seen.borrow(), vec![0, 1], "the owner deregistered it");
    }

    /// T-1.3's guarantee, for the owner stack. A panic mid-build that leaves the stack deeper than it started puts every later creation under the wrong owner — and a lifetime error surfaces nowhere near the panic that caused it.
    #[test]
    fn a_panic_mid_build_leaves_the_stack_where_it_found_it() {
        let before = current_owner();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let _scope = owner_scope();
            let _nested = owner_scope();
            panic!("boom");
        }));

        assert!(outcome.is_err());
        assert_eq!(current_owner(), before);

        let scope = owner_scope();
        let after_panic = create_signal_storage(3i32);
        let owner = scope.id();
        drop(scope);
        dispose_owner(owner);
        assert!(!alive(after_panic), "and the tree is usable afterwards");
    }

    #[test]
    fn a_surface_disposes_the_owner_roots_stamped_with_it() {
        let surface = SurfaceHandle(41);
        // `SurfaceHandle::enter` delegates to a hook a higher crate installs, and with none installed it leaves the current surface alone, so the stamp has to be set through the primitive the hook itself calls.
        let held = {
            let previous = crate::set_current_surface(surface);
            let scope = owner_scope();
            let held = create_signal_storage(4i32);
            drop(scope);
            crate::set_current_surface(previous);
            held
        };
        let elsewhere = {
            let scope = owner_scope();
            let elsewhere = create_signal_storage(5i32);
            drop(scope);
            elsewhere
        };

        dispose_surface_owners(surface);

        assert!(!alive(held));
        assert!(alive(elsewhere), "another surface's owners are untouched");
    }

    /// The allocation shape the frame budget guards. An owner that holds nothing must cost its struct and no more, or a list render turns into thousands of allocations for owners that never hold anything.
    #[test]
    fn an_owner_that_holds_nothing_allocates_nothing() {
        let scope = owner_scope();
        let owner = scope.id();
        drop(scope);

        RUNTIME.with(|rt| {
            let rt = rt.borrow();
            let entry = &rt.owners[owner];
            assert_eq!(entry.children.capacity(), 0);
            assert_eq!(entry.signals.capacity(), 0);
            assert_eq!(entry.effects.capacity(), 0);
        });
        dispose_owner(owner);
    }

    /// A teardown must not run effects over what it has already freed.
    ///
    /// `dispose_surface_owners` frees one root at a time, and a root's cleanups run inside a batch that ends in a flush. An effect that is still registered — its own root's turn has not come yet — re-runs in that flush and reads a memo the previous root already freed, which is a panic in the middle of a window closing rather than an error anyone can act on.
    #[test]
    fn a_teardown_does_not_run_effects_over_what_it_already_freed() {
        let ticks = signal(0i32);
        let read = ticks.read_only();

        let holder = owner_scope();
        let title = memo(move || read.get().to_string());
        let holder_id = holder.id();
        drop(holder);

        let reader = owner_scope();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_c = Rc::clone(&seen);
        effect(move || {
            let _ = read.get();
            seen_c.borrow_mut().push(title.get());
        });
        on_cleanup(move || ticks.set(1));
        let reader_id = reader.id();
        drop(reader);

        assert_eq!(*seen.borrow(), vec!["0".to_string()]);

        dispose_owner(holder_id);
        dispose_owner(reader_id);

        assert_eq!(
            *seen.borrow(),
            vec!["0".to_string()],
            "the effect did not run again over a memo that was already gone"
        );
    }

    /// The same teardown, through the door a closing window actually uses. `dispose_surface_owners` frees the surface's roots one after another, and the guard has to span the set rather than each root: between two of them is exactly where the flush used to land.
    #[test]
    fn a_surface_teardown_does_not_run_effects_over_what_it_already_freed() {
        let ticks = crate::detached(|| signal(0i32));
        let read = ticks.read_only();

        let holder = owner_scope();
        let title = memo(move || read.get().to_string());
        drop(holder);

        let reader = owner_scope();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_c = Rc::clone(&seen);
        effect(move || {
            let _ = read.get();
            seen_c.borrow_mut().push(title.get());
        });
        on_cleanup(move || ticks.set(1));
        drop(reader);

        dispose_surface_owners(current_surface());

        assert_eq!(*seen.borrow(), vec!["0".to_string()]);
    }
}
