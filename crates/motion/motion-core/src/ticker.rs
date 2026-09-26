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
    /// Whether the user's reduced-motion preference may cut this short. Momentum a gesture set going answers `false`: it is the content following the hand, and every platform keeps it when motion is reduced.
    fn reducible(&self) -> bool {
        true
    }
}

struct Registry {
    // Weak so a dropped/unmounted `Animated` deregisters itself; keyed by animation id for idempotent re-registration.
    entries: HashMap<u64, Weak<dyn Tickable>>,
    next_id: u64,
    // Global time scale (D5): 1.0 normal, 0.0 = jump instantly to targets, in-between = slow motion.
    scale: f32,
    follow_reduced_motion: bool,
    // Here rather than in their own registry: the runner asks the same question of both, and this one already crosses the hot-reload FFI boundary a second registry would have to duplicate.
    continuous: u32,
}

impl Registry {
    fn new() -> Self {
        Registry {
            entries: HashMap::new(),
            next_id: 0,
            scale: 1.0,
            follow_reduced_motion: true,
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
    let (scale, follow, live): (f32, bool, Vec<std::rc::Rc<dyn Tickable>>) = REGISTRY.with(|r| {
        let reg = r.borrow();
        let live = reg.entries.values().filter_map(Weak::upgrade).collect();
        (reg.scale, reg.follow_reduced_motion, live)
    });
    let reduce = follow && preferences_core::reduced_motion() == Some(true);
    for anim in &live {
        let scale = if reduce && anim.reducible() {
            0.0
        } else {
            scale
        };
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

/// The time scale [`set_scale`] last set. Reduced motion does not change it: it zeroes the scale [`tick`] hands each animation instead, so an application's own scale is still there when the user turns the preference off.
pub fn scale() -> f32 {
    REGISTRY.with(|r| r.borrow().scale)
}

/// Whether the user's reduced-motion preference zeroes the time scale, which it does unless an application says otherwise.
///
/// With it on and the preference set, every animation jumps to its end as a zero [`set_scale`] would, while momentum a gesture set going (see [`Tickable::reducible`]) keeps moving. Turn it off only for an application that tones its motion down itself, by reading `use_reduced_motion` and choosing gentler animations: an application that simply prefers its animations is overriding the user.
pub fn follow_reduced_motion(follow: bool) {
    REGISTRY.with(|r| r.borrow_mut().follow_reduced_motion = follow);
}

/// Whether [`follow_reduced_motion`] is on.
pub fn follows_reduced_motion() -> bool {
    REGISTRY.with(|r| r.borrow().follow_reduced_motion)
}

#[cfg(test)]
#[path = "ticker_test.rs"]
mod tests;
