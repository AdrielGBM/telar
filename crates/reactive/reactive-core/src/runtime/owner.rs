//! The owner tree disposes reactive state; an owner belongs to exactly one surface, and disposing a surface disposes its owner roots, never the reverse.

use std::any::{Any, TypeId};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::flush::{batch, flush_when_settled};
use super::surface::current_surface;
use super::{EffectId, RUNTIME, Runtime, SignalId, SurfaceHandle};
use crate::runtime::effects::deregister_effect;

slotmap::new_key_type! {
    /// A node in the owner tree: the scope a signal, memo or effect was created in, and what disposes it.
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
    catch: Option<PanicCatch>,
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
            catch: None,
            surface,
        }
    }
}

/// The owner a signal or effect created right now would belong to, or `None` outside every owner scope (creation still falls back to the surface root via `owning_id`).
pub fn current_owner() -> Option<OwnerId> {
    RUNTIME.with(|rt| rt.borrow().owner_stack.last().copied())
}

/// Whether `owner` is `ancestor` or sits anywhere beneath it. `false` once either has been disposed.
pub fn owner_within(owner: OwnerId, ancestor: OwnerId) -> bool {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let mut at = Some(owner);
        while let Some(id) = at {
            if id == ancestor {
                return true;
            }
            at = rt.owners.get(id).and_then(|entry| entry.parent);
        }
        false
    })
}

/// Opens a fresh owner as a child of the active one (the surface root if the stack is empty, matching [`provide_context`]/[`with_context`]) and makes it current until the guard drops.
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

/// Restores the owner scope that was active before [`owner_scope`], truncating to the recorded depth so an unwind through open inner scopes still lands the stack where it started.
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

/// The owner a creation right now belongs to, minting the active surface's root (including the ambient `SurfaceHandle::NONE`) if the stack is empty so nothing created outside a scope ends up unowned; `None` means only [`detached`](reactive_local::detached) state.
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

/// Runs `f` under `owner`, for code such as an event handler that executes long after the build that made it and would otherwise resolve context against the surface root instead of its own component.
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

/// Puts `value` in the current owner's context, replacing whatever this owner had of that type — a rebuild builds its content again and says the same things about it.
pub fn provide_context<T: 'static>(value: T) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(entry) = owning(&mut rt) {
            entry.context.insert(TypeId::of::<T>(), Rc::new(value));
        }
    });
}

/// Reads the nearest value of type `T` at or above the current owner, walking rather than copying so opening a scope stays free and the cost lands on the read instead.
pub fn with_context<T: 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    // Cloned out before the borrow is released, because `f` is the caller's and may read a signal, which would re-enter the runtime and abort.
    let (found, _) = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let from = owning_id(&mut rt);
        context_from(&rt, from, TypeId::of::<T>())
    })?;
    found.downcast_ref::<T>().map(f)
}

/// Reads the nearest value of type `T` at or above the current owner that `f` accepts, walking past the ones it turns down; `f` runs with the runtime released, so it may read a signal.
pub fn find_context<T: 'static, R>(mut f: impl FnMut(&T) -> Option<R>) -> Option<R> {
    let wanted = TypeId::of::<T>();
    let mut next = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let from = owning_id(&mut rt);
        context_from(&rt, from, wanted)
    });
    while let Some((found, above)) = next {
        if let Some(answer) = found.downcast_ref::<T>().and_then(&mut f) {
            return Some(answer);
        }
        next = RUNTIME.with(|rt| context_from(&rt.borrow(), above, wanted));
    }
    None
}

/// The nearest context of type `wanted` at or above `from`, with the owner above the one that holds it.
fn context_from(
    rt: &Runtime,
    mut at: Option<OwnerId>,
    wanted: TypeId,
) -> Option<(Rc<dyn Any>, Option<OwnerId>)> {
    while let Some(id) = at {
        let entry = rt.owners.get(id)?;
        if let Some(value) = entry.context.get(&wanted) {
            return Some((Rc::clone(value), entry.parent));
        }
        at = entry.parent;
    }
    None
}

/// Registers work the current owner runs when it is disposed; outside every owner the closure is dropped unrun, since nothing will ever dispose it.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if let Some(entry) = owning(&mut rt) {
            entry.cleanups.push(Box::new(f));
        }
    });
}

/// What `catch_unwind` hands back for a panic.
pub type PanicPayload = Box<dyn Any + Send>;

/// What an owner does with a panic unwinding out of an effect it holds, or one held by an owner below it — the nearest owner above the effect with a catch answers for that subtree, since the write that re-runs the effect is not where the panic belongs.
#[derive(Clone)]
pub enum PanicCatch {
    /// Keep unwinding: a build of this scope is still on the stack and catches the panic itself, and handling it here would tear the scope down underneath that build.
    Unwind,
    /// `Err` hands the payload back, to be offered to the owners above.
    Handle(Rc<dyn Fn(PanicPayload) -> Result<(), PanicPayload>>),
}

/// Sets what `owner` does with a panic out of an effect at or below it. `None` passes such a panic on to its parent.
pub fn set_panic_catch(owner: OwnerId, catch: Option<PanicCatch>) {
    RUNTIME.with(|rt| {
        if let Some(entry) = rt.borrow_mut().owners.get_mut(owner) {
            entry.catch = catch;
        }
    });
}

/// Offers a panic out of an effect held by `owner` to the nearest owner that catches, resuming the unwind when none does; inside another effect's run it always resumes, since the catching owner may hold that still-executing outer effect.
pub(crate) fn deliver_panic(owner: Option<OwnerId>, mut payload: PanicPayload) {
    if RUNTIME.with(|rt| rt.borrow().running_effects > 0) {
        resume_unwind(payload);
    }
    let mut from = owner;
    loop {
        let found = RUNTIME.with(|rt| {
            let rt = rt.borrow();
            let mut at = from;
            while let Some(id) = at {
                let entry = rt.owners.get(id)?;
                if let Some(catch) = &entry.catch {
                    return Some((catch.clone(), entry.parent));
                }
                at = entry.parent;
            }
            None
        });
        let Some((PanicCatch::Handle(handle), parent)) = found else {
            resume_unwind(payload);
        };
        match handle(payload) {
            Ok(()) => return,
            Err(unhandled) => {
                payload = unhandled;
                from = parent;
            }
        }
    }
}

fn contain(first: &mut Option<PanicPayload>, f: impl FnOnce()) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(f)) {
        first.get_or_insert(payload);
    }
}

/// Raised for as long as a teardown is mid-flight (owner entries removed but effects and signals not yet cleared), so nothing flushes against a half-torn-down tree; counted rather than a flag because disposal nests.
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

/// Disposes an owner and everything below it, children first; a panic mid-teardown resumes only once it completes, and whatever the cleanups invalidated is flushed afterward rather than dropped, since a surviving effect may still need to see it.
pub fn dispose_owner(id: OwnerId) {
    let (cleanups, effects, signals) = uproot(id);
    let mut panicked = None;

    {
        let _disposing = Disposing::enter();

        // One wave, not one per withdrawal: `undeclare` bumps the cascade's `structure` signal, which every context read subscribes to, so tearing down N nodes outside a batch is N invalidations across the tree.
        batch(|| {
            for cleanup in cleanups {
                contain(&mut panicked, cleanup);
            }
        });

        for effect in effects {
            contain(&mut panicked, || deregister_effect(effect));
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
        contain(&mut panicked, || drop(removed));
    }

    flush_when_settled();
    if let Some(payload) = panicked {
        resume_unwind(payload);
    }
}

/// Runs `f` with what it creates belonging to the active surface's world: state that lives exactly as long as the surface, for a surface's own worlds (its layout tree, cascade, exit count) that must outlive every scope inside it and be freed when it is, which nothing [`detached`](crate::detached) ever is.
pub fn in_surface_world<R>(f: impl FnOnce() -> R) -> R {
    let surface = current_surface();
    let depth = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let world = match rt.worlds.get(&surface).copied() {
            Some(world) if rt.owners.contains_key(world) => world,
            _ => {
                let world = rt.owners.insert(OwnerEntry::new(None, surface));
                rt.worlds.insert(surface, world);
                world
            }
        };
        let depth = rt.owner_stack.len();
        rt.owner_stack.push(world);
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

/// Disposes everything a surface holds: its owner roots, then its world — the world goes last because the roots' cleanups still write into it.
pub fn dispose_surface(surface: SurfaceHandle) {
    RUNTIME.with(|rt| rt.borrow_mut().disposed_surfaces.insert(surface));
    let mut panicked = None;
    {
        let _disposing = Disposing::enter();
        contain(&mut panicked, || dispose_surface_owners(surface));
        let world = RUNTIME.with(|rt| rt.borrow_mut().worlds.remove(&surface));
        if let Some(world) = world {
            contain(&mut panicked, || dispose_owner(world));
        }
    }
    flush_when_settled();
    if let Some(payload) = panicked {
        resume_unwind(payload);
    }
}

/// Whether [`dispose_surface`] has already torn this surface down. [`SurfaceHandle::NONE`], the ambient surface, is never disposed.
pub(crate) fn is_surface_disposed(surface: SurfaceHandle) -> bool {
    !surface.is_none() && RUNTIME.with(|rt| rt.borrow().disposed_surfaces.contains(&surface))
}

/// Disposes every owner root belonging to a surface, leaving its world standing: see [`dispose_surface`].
pub fn dispose_surface_owners(surface: SurfaceHandle) {
    let roots: Vec<OwnerId> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.roots.remove(&surface);
        let world = rt.worlds.get(&surface).copied();
        rt.owners
            .iter()
            .filter(|(id, entry)| {
                entry.parent.is_none() && entry.surface == surface && Some(*id) != world
            })
            .map(|(id, _)| id)
            .collect()
    });
    let mut panicked = None;
    {
        // Held across the whole set rather than left to each root: the roots of one surface read each other, so a flush landing between two of them runs effects over what the one before it just freed.
        let _disposing = Disposing::enter();
        for root in roots {
            contain(&mut panicked, || dispose_owner(root));
        }
    }
    flush_when_settled();
    if let Some(payload) = panicked {
        resume_unwind(payload);
    }
}

/// How many signals the runtime is holding — the arenas are private, so this is the only evidence a test has that disposal actually freed something rather than just re-rendering.
pub fn live_signal_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().signals.len())
}

/// How many effects the runtime is holding. See [`live_signal_count`].
pub fn live_effect_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().effects.len())
}

/// How many owners the runtime is holding. See [`live_signal_count`].
pub fn live_owner_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().owners.len())
}

/// Removes an owner's subtree from the arena and hands back what it held, deepest owner first; iterative rather than recursive, since a stack overflow inside disposal would abort rather than unwind.
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
#[path = "owner_test.rs"]
mod tests;

#[cfg(test)]
#[path = "panic_test.rs"]
mod panic_tests;
