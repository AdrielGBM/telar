//! The per-thread registry of live animations, and the single `tick` the runner drives them all from.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Weak;
use web_time::Instant;

/// Something that advances with the frame clock, erased over whatever it is advancing.
///
/// `Animated` is the usual one, and not the only kind there is: a value eased between two endpoints is one thing, and a velocity that decays until it runs out or meets a bound is another. A scroll fling is the second — it has no target until it stops — so the registry takes anything that can be told the time rather than only animations.
pub trait Tickable {
    /// Advance to `now`. `scale` is the global time scale: `0.0` means jump straight to the end.
    fn tick(&self, now: Instant, scale: f32);
    /// Whether there is nothing left to advance, at which point the registry lets go of it.
    fn is_settled(&self) -> bool;
}

struct Registry {
    // Weak so a dropped/unmounted `Animated` deregisters itself; keyed by animation id for idempotent re-registration.
    entries: HashMap<u64, Weak<dyn Tickable>>,
    next_id: u64,
    // Global time scale (D5): 1.0 normal, 0.0 = jump instantly to targets, in-between = slow motion.
    scale: f32,
    // Here rather than in their own registry: the runner asks the same question of both, and this one already crosses the hot-reload FFI boundary a second registry would have to duplicate.
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

// `ManuallyDrop` keeps this TLS trivially destructible: a destructor registered from the app dylib would make `dlclose` unsafe. The map leaks per reload, which is fine on a dev-only path.
thread_local! {
    static REGISTRY: ManuallyDrop<RefCell<Registry>> = ManuallyDrop::new(RefCell::new(Registry::new()));
}

/// An identity for one registration, so re-registering the same thing replaces rather than duplicates it.
pub fn next_id() -> u64 {
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        let id = reg.next_id;
        reg.next_id += 1;
        id
    })
}

/// Registers something to be advanced once per frame.
///
/// Held weakly, so whatever registered it deregisters by being dropped and nothing has to remember to say so. Ticking is the frame's, and a value written from `tick` is a write like any other — the runner calls it outside the render, which is why an animation may publish into a signal from there at all.
pub fn register(id: u64, weak: Weak<dyn Tickable>) {
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
/// An animation moves values Telar owns, so the tree reports itself dirty and the loop schedules the next frame on its own. A region filled from outside — a texture the application renders into (`telar::gpu::image`), a video decoding on another thread — changes no value here at all: the draw commands are identical every frame while the picture underneath is not. Nothing in the tree can notice that, which is why it has to be declared.
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

/// Whether any [`Continuous`] region is alive. The runner keeps scheduling frames while it is true, and moves the content generation so the renderer cannot mistake identical commands for an identical frame.
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
