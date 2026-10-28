//! The owner tree disposes reactive state; an owner belongs to exactly one surface, and disposing a surface disposes its owner roots, never the reverse, and never one made inside [`detached`](reactive_local::detached).

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use rustc_hash::FxHashMap;

use super::flush::{batch, flush_when_settled};
use super::surface::current_surface;
use super::{EffectId, RUNTIME, Runtime, SignalId, SurfaceHandle};
use crate::runtime::effects::deregister_effect;

slotmap::new_key_type! {
    pub(crate) struct OwnerKey;
}

/// A node in the owner tree, the scope a signal, memo or effect was created in and what disposes it; `!Send` because its key names an owner only in the creating thread's runtime.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerId {
    key: OwnerKey,
    _ui_thread_only: PhantomData<*const ()>,
}

impl std::fmt::Debug for OwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OwnerId")
            .field(&slotmap::Key::data(&self.key))
            .finish()
    }
}

impl From<OwnerKey> for OwnerId {
    fn from(key: OwnerKey) -> Self {
        OwnerId {
            key,
            _ui_thread_only: PhantomData,
        }
    }
}

pub(crate) struct OwnerEntry {
    parent: Option<OwnerKey>,
    children: Vec<OwnerKey>,
    signals: Vec<SignalId>,
    effects: Vec<EffectId>,
    /// What this owner has to undo elsewhere. The escape hatch for state an owner's lifetime governs but this crate cannot name — a cascade declaration is keyed by a layout `NodeId`, which lives three crates up.
    cleanups: Vec<Box<dyn FnOnce()>>,
    /// What this owner tells everything below it, keyed by type. Empty for almost every owner: only a scope that provides something ever reaches the allocator for this.
    context: FxHashMap<TypeId, Rc<dyn Any>>,
    catch: Option<PanicCatch>,
    surface: SurfaceHandle,
    /// Made inside [`detached`](reactive_local::detached): a root no surface's teardown frees, because what it holds is reached for long after the surface active when it was made, which `surface` still names so its own teardown runs there.
    detached: bool,
}

impl OwnerEntry {
    // A 10k-row list is 10k of these, so an owner holding nothing must not reach the allocator.
    fn new(parent: Option<OwnerKey>, surface: SurfaceHandle) -> Self {
        OwnerEntry {
            parent,
            children: Vec::new(),
            signals: Vec::new(),
            effects: Vec::new(),
            cleanups: Vec::new(),
            context: FxHashMap::default(),
            catch: None,
            surface,
            detached: false,
        }
    }

    fn root(surface: SurfaceHandle) -> Self {
        OwnerEntry {
            detached: reactive_local::is_detached(),
            ..OwnerEntry::new(None, surface)
        }
    }
}

/// The owner a signal or effect created right now would belong to, or `None` outside every owner scope (creation still falls back to the surface root via `owning_id`).
pub fn current_owner() -> Option<OwnerId> {
    RUNTIME.with(|rt| rt.borrow().owner_stack.last().copied().map(OwnerId::from))
}

/// Whether `owner` is `ancestor` or sits anywhere beneath it. `false` once either has been disposed.
pub fn owner_within(owner: OwnerId, ancestor: OwnerId) -> bool {
    RUNTIME.with(|rt| {
        let rt = rt.borrow();
        let mut at = Some(owner.key);
        while let Some(id) = at {
            if id == ancestor.key {
                return true;
            }
            at = rt.owners.get(id).and_then(|entry| entry.parent);
        }
        false
    })
}

/// Opens a fresh owner as a child of the active one (the surface root if the stack is empty, matching [`provide_context`]/[`with_context`]) and makes it current until the guard drops; inside [`detached`](reactive_local::detached) it is a root no surface's teardown frees, for state a thread-local or a cache hands out long after this build.
pub fn owner_scope() -> OwnerGuard {
    let surface = current_surface();
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let entry = match owning_id(&mut rt) {
            Some(parent) => OwnerEntry::new(Some(parent), surface),
            None => OwnerEntry::root(surface),
        };
        let parent = entry.parent;
        let id = rt.owners.insert(entry);
        if let Some(parent) = parent
            && let Some(entry) = rt.owners.get_mut(parent)
        {
            entry.children.push(id);
        }
        let depth = rt.owner_stack.len();
        rt.owner_stack.push(id);
        OwnerGuard {
            id: id.into(),
            depth,
        }
    })
}

/// Opens a fresh owner with no parent, belonging to the active surface, and makes it current until the guard drops.
///
/// For what lives exactly as long as a surface, wherever the call happens: a surface nested in another tree is built from inside some component's scope, and as that scope's child its content would be freed with the component rather than with the surface, and would read the context the component's ancestors provide as though it were its own. [`dispose_surface`] frees it with the surface's other roots, unless it was opened inside [`detached`](reactive_local::detached).
pub fn root_scope() -> OwnerGuard {
    let surface = current_surface();
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        let id = rt.owners.insert(OwnerEntry::root(surface));
        let depth = rt.owner_stack.len();
        rt.owner_stack.push(id);
        OwnerGuard {
            id: id.into(),
            depth,
        }
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

pub(crate) fn owning_id(rt: &mut Runtime) -> Option<OwnerKey> {
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
            && rt.owners.contains_key(owner.key)
        {
            rt.owner_stack.push(owner.key);
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
    let value: Rc<dyn Any> = Rc::new(value);
    // Dropped after the borrow is released: the value is the caller's, and its `Drop` may reach the runtime.
    let replaced = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        match owning(&mut rt) {
            Some(entry) => entry.context.insert(TypeId::of::<T>(), value),
            None => Some(value),
        }
    });
    drop(replaced);
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
    mut at: Option<OwnerKey>,
    wanted: TypeId,
) -> Option<(Rc<dyn Any>, Option<OwnerKey>)> {
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
    let replaced = RUNTIME.with(|rt| match rt.borrow_mut().owners.get_mut(owner.key) {
        Some(entry) => std::mem::replace(&mut entry.catch, catch),
        None => catch,
    });
    drop(replaced);
}

/// Offers a panic out of an effect held by `owner` to the nearest owner that catches, resuming the unwind when none does; inside another effect's run it always resumes, since the catching owner may hold that still-executing outer effect.
pub(crate) fn deliver_panic(owner: Option<OwnerKey>, mut payload: PanicPayload) {
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

/// Raised while a teardown's body runs, so an owner disposed from inside it — by a cleanup, or by a widget dropping with an effect — waits for it instead of being freed under it: set only by the outermost [`dispose_owner`], since every one nested in it is deferred.
struct TearingDown;

impl TearingDown {
    fn enter() -> Self {
        RUNTIME.with(|rt| rt.borrow_mut().tearing_down = true);
        TearingDown
    }
}

impl Drop for TearingDown {
    fn drop(&mut self) {
        RUNTIME.with(|rt| rt.borrow_mut().tearing_down = false);
    }
}

/// Disposes an owner and everything below it, children first, each owner's teardown inside the surface it was created on; a panic mid-teardown resumes only once it completes, and whatever the cleanups invalidated is flushed afterward rather than dropped, since a surviving effect may still need to see it. Called from inside another teardown, the owner is freed once that teardown's widgets are gone but before its signals are: a cleanup that lets go of state the torn tree's widgets still read as they drop, a gesture cancelled by its box going away, would otherwise free it under them.
pub fn dispose_owner(id: OwnerId) {
    let id = id.key;
    let deferred = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if rt.tearing_down {
            rt.released_mid_teardown.push(id);
        }
        rt.tearing_down
    });
    if deferred {
        return;
    }
    let mut panicked = None;
    {
        let _disposing = Disposing::enter();
        let _tearing = TearingDown::enter();
        tear_down(id, &mut panicked);
    }
    flush_when_settled();
    resume_unless_unwinding(panicked);
}

fn tear_down(id: OwnerKey, panicked: &mut Option<PanicPayload>) {
    let mut torn = uproot(id);

    // One wave, not one per withdrawal: `undeclare` bumps the cascade's `structure` signal, which every context read subscribes to, so tearing down N nodes outside a batch is N invalidations across the tree.
    batch(|| {
        for owner in &mut torn {
            owner.surface.with_entered(|| {
                for cleanup in std::mem::take(&mut owner.cleanups) {
                    contain(panicked, cleanup);
                }
            });
        }
    });

    for owner in &mut torn {
        owner.surface.with_entered(|| {
            for effect in std::mem::take(&mut owner.effects) {
                contain(panicked, || deregister_effect(effect));
            }
            // Before the signals go, since what it provides may hold them.
            let provided = (std::mem::take(&mut owner.context), owner.catch.take());
            contain(panicked, || drop(provided));
        });
    }
    free_released_mid_teardown(panicked);
    for owner in torn {
        // Taken out under the borrow and dropped after releasing it: a signal whose value owns signal handles re-enters the runtime as that value drops, which under the borrow aborts.
        let removed: Vec<Rc<dyn Any>> = RUNTIME.with(|rt| {
            let mut rt = rt.borrow_mut();
            owner
                .signals
                .into_iter()
                .filter_map(|signal| rt.signals.remove(signal))
                .map(|storage| storage.value)
                .collect()
        });
        owner
            .surface
            .with_entered(|| contain(panicked, || drop(removed)));
    }
}

fn free_released_mid_teardown(panicked: &mut Option<PanicPayload>) {
    loop {
        let released =
            RUNTIME.with(|rt| std::mem::take(&mut rt.borrow_mut().released_mid_teardown));
        if released.is_empty() {
            return;
        }
        for id in released {
            tear_down(id, panicked);
        }
    }
}

/// A second panic resumed out of a `Drop` that runs while the thread already unwinds aborts the process, hiding the first; that one is already on its way out.
fn resume_unless_unwinding(panicked: Option<PanicPayload>) {
    if let Some(payload) = panicked
        && !std::thread::panicking()
    {
        resume_unwind(payload);
    }
}

/// Runs `f` with what it creates belonging to the active surface's world: state that lives exactly as long as the surface, for a surface's own worlds (its layout tree, cascade, exit count) that must outlive every scope inside it and be freed when it is, which nothing [`detached`](crate::detached) ever is — owners opened there included, since their handles outlive the surface that was active.
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

/// Disposes everything a surface holds: its owner roots, then its world — the world goes last because the roots' cleanups still write into it — but no owner opened inside [`detached`](crate::detached), whose handles outlive the surface that was active when it was opened.
pub fn dispose_surface(surface: SurfaceHandle) {
    RUNTIME.with(|rt| rt.borrow_mut().disposed_surfaces.insert(surface));
    let mut panicked = None;
    {
        let _disposing = Disposing::enter();
        contain(&mut panicked, || dispose_surface_owners(surface));
        let world = RUNTIME.with(|rt| rt.borrow_mut().worlds.remove(&surface));
        if let Some(world) = world {
            contain(&mut panicked, || dispose_owner(world.into()));
        }
    }
    flush_when_settled();
    resume_unless_unwinding(panicked);
}

/// Whether [`dispose_surface`] has already torn this surface down. [`SurfaceHandle::NONE`], the ambient surface, is never disposed.
pub(crate) fn is_surface_disposed(surface: SurfaceHandle) -> bool {
    !surface.is_none() && RUNTIME.with(|rt| rt.borrow().disposed_surfaces.contains(&surface))
}

/// Disposes every owner root belonging to a surface, leaving its world standing: see [`dispose_surface`].
pub fn dispose_surface_owners(surface: SurfaceHandle) {
    let roots: Vec<OwnerKey> = RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        rt.roots.remove(&surface);
        let world = rt.worlds.get(&surface).copied();
        rt.owners
            .iter()
            .filter(|(id, entry)| {
                entry.parent.is_none()
                    && !entry.detached
                    && entry.surface == surface
                    && Some(*id) != world
            })
            .map(|(id, _)| id)
            .collect()
    });
    let mut panicked = None;
    {
        // Held across the whole set rather than left to each root: the roots of one surface read each other, so a flush landing between two of them runs effects over what the one before it just freed.
        let _disposing = Disposing::enter();
        for root in roots {
            contain(&mut panicked, || dispose_owner(root.into()));
        }
    }
    flush_when_settled();
    resume_unless_unwinding(panicked);
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

/// Removes an owner's subtree from the arena and hands back its entries, deepest owner first, to be torn down and dropped once the borrow is released; iterative rather than recursive, since a stack overflow inside disposal would abort rather than unwind.
fn uproot(root: OwnerKey) -> Vec<OwnerEntry> {
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

        order
            .into_iter()
            .rev()
            .filter_map(|id| rt.owners.remove(id))
            .collect()
    })
}

#[cfg(test)]
#[path = "owner_test.rs"]
mod tests;

#[cfg(test)]
#[path = "panic_test.rs"]
mod panic_tests;
