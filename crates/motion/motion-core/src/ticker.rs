use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Weak;
use web_time::Instant;

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
    // Live `Continuous` guards. Kept here rather than in their own registry because the runner asks the same question of both — "is anything still in flight?" — and this one already crosses the hot-reload FFI boundary that a second registry would have to duplicate.
    continuous: u32,
}

impl Registry {
    fn new() -> Self {
        Registry {
            entries: HashMap::new(),
            next_id: 0,
            scale: 1.0,
            continuous: 0,
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

/// A registered handle is live if it is still upgradeable and not yet settled.
fn is_live(weak: &Weak<dyn Tickable>) -> bool {
    matches!(weak.upgrade(), Some(anim) if !anim.is_settled())
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
        r.borrow_mut().entries.retain(|_, weak| is_live(weak));
    });
}

/// Whether any animation is still unsettled; the runner uses this to keep scheduling frames. Non-mutating: a Weak can go dead between `tick` and this call, so it re-tests liveness rather than trusting emptiness.
pub fn has_active() -> bool {
    REGISTRY.with(|r| r.borrow().entries.values().any(is_live))
}

/// Keeps frames coming while it lives, for content Telar cannot see changing.
///
/// An animation moves values Telar owns, so the tree reports itself dirty and the loop schedules the next
/// frame on its own. A region filled from outside — a texture the application renders into
/// (`telar::gpu::image`), a video decoding on another thread — changes no value here at all: the draw
/// commands are identical every frame while the picture underneath is not. Nothing in the tree can
/// notice that, which is why it has to be declared.
///
/// Hold one for as long as the region is on screen; dropping it lets the loop go back to sleep.
pub struct Continuous(());

impl Continuous {
    pub fn new() -> Self {
        REGISTRY.with(|r| r.borrow_mut().continuous += 1);
        Self(())
    }
}

impl Default for Continuous {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Continuous {
    fn drop(&mut self) {
        REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            reg.continuous = reg.continuous.saturating_sub(1);
        });
    }
}

/// Whether any [`Continuous`] region is alive. The runner keeps scheduling frames while it is true, and
/// moves the content generation so the renderer cannot mistake identical commands for an identical frame.
pub fn has_continuous() -> bool {
    REGISTRY.with(|r| r.borrow().continuous > 0)
}

/// Drop all registered animations; parallels reactive `reset_runtime` on tree teardown / hot reload.
pub fn reset() {
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        reg.entries.clear();
        // The guards themselves live in the tree being torn down, and their `Drop` would saturate at zero against a counter this reset had already cleared. Clearing it here keeps a reload from leaving a phantom region scheduling frames forever.
        reg.continuous = 0;
    });
}

/// Set the global time scale (D5). 1.0 is normal; 0.0 makes animations jump instantly to their targets; values in between slow motion down. Negative inputs clamp to 0.0.
pub fn set_scale(scale: f32) {
    REGISTRY.with(|r| r.borrow_mut().scale = scale.max(0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    // The registry is thread-local and libtest reuses threads; each test starts from a known state.
    fn fresh() {
        reset();
    }

    #[test]
    fn a_live_guard_keeps_the_loop_awake() {
        fresh();
        assert!(!has_continuous());
        let region = Continuous::new();
        assert!(has_continuous());
        drop(region);
        assert!(!has_continuous());
    }

    // Two viewports on screen at once: the loop sleeps when the last one goes, not the first.
    #[test]
    fn the_loop_sleeps_only_when_the_last_region_goes() {
        fresh();
        let first = Continuous::new();
        let second = Continuous::new();
        drop(first);
        assert!(has_continuous(), "one region is still on screen");
        drop(second);
        assert!(!has_continuous());
    }

    // A guard lives in the tree a reload tears down, so its Drop runs against a counter already cleared. Saturating there rather than wrapping is what keeps a reload from leaving a phantom region behind.
    #[test]
    fn a_reload_leaves_no_phantom_region_scheduling_frames() {
        fresh();
        let region = Continuous::new();
        reset();
        assert!(!has_continuous());
        drop(region);
        assert!(!has_continuous(), "the counter must not wrap below zero");
    }

    // Animations settle and continuous regions do not; the runner asks the two questions separately because only the second one has to move the content generation.
    #[test]
    fn a_continuous_region_is_not_an_active_animation() {
        fresh();
        let _region = Continuous::new();
        assert!(has_continuous());
        assert!(!has_active(), "no animation was registered");
    }
}
