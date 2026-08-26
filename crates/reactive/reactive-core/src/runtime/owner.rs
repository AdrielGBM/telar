//! The owner tree — what disposes reactive state, once anything does.
//!
//! Every signal, memo and effect belongs to a node in a tree, recorded at creation from whatever owner is
//! active, the same way [`EffectEntry::surface`](super::EffectEntry) records the surface. Disposing an
//! owner walks its children first, then frees what it holds itself. Nothing mints an owner yet; this is
//! the tree the later phases hang lifetimes on.
//!
//! # How this composes with `surface_local!`
//!
//! **A surface is the outer world; an owner is a node in the inner tree.** They are not competing scoping
//! mechanisms and must not be read as one.
//!
//! A surface owns a set of swappable thread-local worlds — its layout tree, service stack, overlays,
//! focus, input region, window-command queue (`ui-core/src/surface_context.rs`). Those keep their job
//! exactly as it is. An owner owns *reactive state*: the entries in this runtime's arenas. An owner
//! belongs to exactly one surface, the one active when it was created, and disposing a surface disposes
//! the owner roots stamped with it — never the reverse. Owners nest inside a surface; a surface never
//! nests inside an owner.
//!
//! The reason to write it down: both answer "who cleans this up", so the temptation is to fold one into
//! the other. They clean up different things, on different schedules — a surface lives for a window, an
//! owner for a component instance, an `if` branch, or one row of a list.

use std::any::Any;

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
    surface: SurfaceHandle,
}

impl OwnerEntry {
    // A 10k-row list is 10k of these, so an owner that holds nothing must not reach the allocator. A field that allocates eagerly turns a list render into thousands of allocations.
    fn new(parent: Option<OwnerId>, surface: SurfaceHandle) -> Self {
        OwnerEntry {
            parent,
            children: Vec::new(),
            signals: Vec::new(),
            effects: Vec::new(),
            surface,
        }
    }
}

/// The owner a signal or effect created right now would belong to, or `None` outside every owner scope.
///
/// Reactive state created outside an owner is recorded nowhere and stays governed by the handle refcount,
/// which is exactly today's behaviour. That hole closes when the refcount does; see Decisions 5.
pub fn current_owner() -> Option<OwnerId> {
    RUNTIME.with(|rt| rt.borrow().owner_stack.last().copied())
}

/// Opens a fresh owner as a child of the active one and makes it current until the guard drops.
pub fn owner_scope() -> OwnerGuard {
    let parent = current_owner();
    let surface = current_surface();
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
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
/// Truncates to the depth recorded on entry rather than popping one, so an unwind through a build that
/// left inner scopes open still lands the stack where it started. An unbalanced stack is the worst kind of
/// bug to leave behind: every later creation goes to the wrong owner, and being a lifetime error it
/// surfaces nowhere near the panic that caused it. `batch_depth` meets this standard already.
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

/// The owner a creation right now belongs to, or `None` if it belongs to nobody.
///
/// Nobody covers two different cases. Outside every scope there is no owner to attribute to. Inside
/// [`detached`](reactive_local::detached) there is one and it is the wrong one: a `surface_local!` world
/// initialises on first access, and the first access is somebody's build, so the surface's own state would
/// be adopted by whatever row or branch happened to touch it — and freed when that row went away.
fn owning(rt: &mut Runtime) -> Option<&mut OwnerEntry> {
    if reactive_local::is_detached() {
        return None;
    }
    let owner = rt.owner_stack.last().copied()?;
    rt.owners.get_mut(owner)
}

/// Disposes an owner and everything below it, children first.
pub fn dispose_owner(id: OwnerId) {
    let (effects, signals) = uproot(id);
    for effect in effects {
        deregister_effect(effect);
    }
    // Take the storages out under the borrow and drop them after releasing it. A signal whose value owns signal handles re-enters the runtime as that value drops, and doing it under the borrow aborts — the same hazard `drop_signal` carries a note about.
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
    for root in roots {
        dispose_owner(root);
    }
}

/// How many signals the runtime is holding.
///
/// The arenas are private, so a lifetime that nothing frees is otherwise invisible — a test can watch a
/// row rebuild ten times and see only that it still renders. Owner-scoped disposal makes the count the
/// evidence: it is flat when disposal runs, and climbs when it does not.
pub fn live_signal_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().signals.len())
}

/// How many effects the runtime is holding. See [`live_signal_count`].
pub fn live_effect_count() -> usize {
    RUNTIME.with(|rt| rt.borrow().effects.len())
}

/// Removes an owner's subtree from the arena and hands back what it held, deepest owner first.
///
/// Iterative rather than recursive: the depth is the component nesting of a real view, and a stack
/// overflow inside disposal would abort rather than unwind.
fn uproot(root: OwnerId) -> (Vec<EffectId>, Vec<SignalId>) {
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

        let mut effects = Vec::new();
        let mut signals = Vec::new();
        for id in order.into_iter().rev() {
            if let Some(entry) = rt.owners.remove(id) {
                effects.extend(entry.effects);
                signals.extend(entry.signals);
            }
        }
        (effects, signals)
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    use super::*;
    use crate::runtime::signals::create_signal_storage;
    use crate::{effect, signal};

    fn alive(id: SignalId) -> bool {
        RUNTIME.with(|rt| rt.borrow().signals.contains_key(id))
    }

    #[test]
    fn disposing_an_owner_takes_its_children_with_it() {
        let outer = owner_scope();
        let held_by_outer = create_signal_storage(1i32, 1);
        let held_by_inner = {
            let _inner = owner_scope();
            create_signal_storage(2i32, 1)
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
        let handle = effect(move || seen_c.borrow_mut().push(read.get()));
        let owner = scope.id();
        drop(scope);

        count.set(1);
        assert_eq!(*seen.borrow(), vec![0, 1]);

        dispose_owner(owner);
        count.set(2);
        assert_eq!(
            *seen.borrow(),
            vec![0, 1],
            "the owner deregistered it, though the handle is still held"
        );
        drop(handle);
    }

    /// T-1.3's guarantee, for the owner stack. A panic mid-build that leaves the stack deeper than it
    /// started puts every later creation under the wrong owner — and a lifetime error surfaces nowhere near
    /// the panic that caused it.
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
        let after_panic = create_signal_storage(3i32, 1);
        let owner = scope.id();
        drop(scope);
        dispose_owner(owner);
        assert!(!alive(after_panic), "and the tree is usable afterwards");
    }

    #[test]
    fn a_surface_disposes_the_owner_roots_stamped_with_it() {
        let surface = SurfaceHandle(41);
        // `SurfaceHandle::enter` delegates to the hook a higher crate installs, and with none installed it is a no-op that leaves the current surface alone — so the stamp has to be set through the primitive the hook itself calls.
        let held = {
            let previous = crate::set_current_surface(surface);
            let scope = owner_scope();
            let held = create_signal_storage(4i32, 1);
            drop(scope);
            crate::set_current_surface(previous);
            held
        };
        let elsewhere = {
            let scope = owner_scope();
            let elsewhere = create_signal_storage(5i32, 1);
            drop(scope);
            elsewhere
        };

        dispose_surface_owners(surface);

        assert!(!alive(held));
        assert!(alive(elsewhere), "another surface's owners are untouched");
    }

    /// The allocation shape the frame budget guards. An owner that holds nothing must cost its struct and
    /// no more, or a list render turns into thousands of allocations for owners that never hold anything.
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
}
