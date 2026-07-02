use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Weak;
use std::time::Instant;

/// Registry-facing behavior of an animation, erased over its value type.
pub(crate) trait Tickable {
    fn tick(&self, now: Instant, scale: f32);
    fn is_settled(&self) -> bool;
}

struct Registry {
    // Weak so a dropped/unmounted `Animated` deregisters itself; keyed by animation id for idempotent re-registration.
    entries: HashMap<u64, Weak<dyn Tickable>>,
    next_id: u64,
    // Global time scale (D5): 1.0 normal, 0.0 = jump instantly to targets, in-between = slow motion.
    scale: f32,
}

impl Registry {
    fn new() -> Self {
        Registry {
            entries: HashMap::new(),
            next_id: 0,
            scale: 1.0,
        }
    }
}

// ManuallyDrop keeps this TLS trivially-destructible: registering a TLS destructor from the app dylib would make dlclose unsafe during hot reload (mirrors reactive-core's runtime and rsx's hot_state). The map leaks per reload, which is fine for a dev-only path; `reset` clears it explicitly on teardown.
thread_local! {
    static REGISTRY: ManuallyDrop<RefCell<Registry>> = ManuallyDrop::new(RefCell::new(Registry::new()));
}

pub(crate) fn next_id() -> u64 {
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        let id = reg.next_id;
        reg.next_id += 1;
        id
    })
}

pub(crate) fn register(id: u64, weak: Weak<dyn Tickable>) {
    REGISTRY.with(|r| {
        r.borrow_mut().entries.insert(id, weak);
    });
}

/// Integrate every active animation to `now`, publishing changed values and deregistering settled ones.
pub fn tick(now: Instant) {
    // Snapshot live handles under a short borrow, then integrate without holding the registry borrow: each `.set()` may flush effects that re-enter the registry (register new animations).
    let (scale, live): (f32, Vec<std::rc::Rc<dyn Tickable>>) = REGISTRY.with(|r| {
        let reg = r.borrow();
        let live = reg.entries.values().filter_map(Weak::upgrade).collect();
        (reg.scale, live)
    });
    for anim in &live {
        anim.tick(now, scale);
    }
    // Prune dead (dropped) and settled animations so has_active() returns false at rest.
    REGISTRY.with(|r| {
        r.borrow_mut()
            .entries
            .retain(|_, weak| matches!(weak.upgrade(), Some(anim) if !anim.is_settled()));
    });
}

/// Whether any animation is still unsettled; the runner uses this to keep scheduling frames.
pub fn has_active() -> bool {
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        reg.entries
            .retain(|_, weak| matches!(weak.upgrade(), Some(anim) if !anim.is_settled()));
        !reg.entries.is_empty()
    })
}

/// Drop all registered animations; parallels reactive `reset_runtime` on tree teardown / hot reload.
pub fn reset() {
    REGISTRY.with(|r| r.borrow_mut().entries.clear());
}

/// Set the global time scale (D5). 1.0 is normal; 0.0 makes animations jump instantly to their targets; values in between slow motion down. Negative inputs clamp to 0.0.
pub fn set_scale(scale: f32) {
    REGISTRY.with(|r| r.borrow_mut().scale = scale.max(0.0));
}
