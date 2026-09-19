//! [`Animated`]: a value that travels to its target under a curve, integrated once per frame by the ticker.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use web_time::Instant;

use reactive_core::{ReadSignal, RwSignal, signal};

use crate::curve::{Curve, Spring, Tween};
use crate::lerp::Lerp;
use crate::ticker::{self, Tickable};

// Retargeting within this squared distance of the current goal is a no-op, so a segment re-running view() (which re-calls retarget with the same source) never perturbs an in-flight animation.
const NOOP_EPS_SQ: f32 = 1e-12;
// Spring integration sub-step; large frame gaps are split into steps this size for stability.
const MAX_SUBSTEP: f32 = 1.0 / 240.0;
// Upper bound on a single integrated frame; caps the jump after a long stall (paused window, breakpoint).
const MAX_FRAME_DT: f32 = 0.1;
// A multi-surface app ticks every animation once per surface per frame, and at those microsecond gaps both integration steps round away in f32, freezing the animation short of settling and pinning `has_active()`.
const MIN_FRAME_DT: f32 = 1.0 / 1000.0;
// Spring settles once both squared displacement and squared velocity fall below these.
const DISP_EPS_SQ: f32 = 1e-6;
const VEL_EPS_SQ: f32 = 1e-6;
const MIN_MASS: f32 = 1e-4;

pub(crate) struct AnimInner<T: Lerp + 'static> {
    signal: RwSignal<T>,
    current: T,
    target: T,
    // Value-space velocity (component-wise); only used by springs.
    velocity: T,
    // Tween origin captured at retarget; the tween lerps start -> target.
    start: T,
    elapsed_secs: f32,
    // Duration of the tween leg currently playing, which may be less than the curve's full duration if it interrupted an in-flight leg.
    leg_secs: f32,
    // Distance covered by the last full-duration leg, the reference a shorter interrupting leg's fraction is computed against.
    full_span: f32,
    curve: Curve,
    settled: bool,
    // Timestamp of the last integration; None means the next tick only establishes t0.
    last: Option<Instant>,
}

impl<T: Lerp + 'static> AnimInner<T> {
    // Integrate to `now` scaled by `scale`; returns the new value to publish, or None if it did not change this tick.
    fn integrate(&mut self, now: Instant, scale: f32) -> Option<T> {
        if self.settled {
            return None;
        }
        // scale <= 0 means reduced-motion "instant": jump to the target and settle (D5).
        if scale <= 0.0 {
            return Some(self.snap_to_target());
        }
        let last = match self.last {
            Some(last) => last,
            None => {
                self.last = Some(now);
                return None;
            }
        };
        let dt = now.saturating_duration_since(last).as_secs_f32() * scale;
        // Before `self.last` advances: a skipped step must leave the clock alone so its time is carried into the next one rather than discarded.
        if dt < MIN_FRAME_DT {
            return None;
        }
        self.last = Some(now);
        match self.curve {
            Curve::Tween(t) => self.step_tween(t, dt),
            Curve::Spring(s) => self.step_spring(s, dt),
        }
    }

    fn step_tween(&mut self, t: Tween, dt: f32) -> Option<T> {
        self.elapsed_secs += dt;
        let duration = self.leg_secs;
        if duration <= 0.0 || self.elapsed_secs >= duration {
            return Some(self.snap_to_target());
        }
        let eased = t.easing.apply(self.elapsed_secs / duration);
        self.current = self.start.lerp(&self.target, eased);
        Some(self.current.clone())
    }

    fn step_spring(&mut self, s: Spring, dt: f32) -> Option<T> {
        let dt = dt.min(MAX_FRAME_DT);
        let steps = (dt / MAX_SUBSTEP).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        let mass = s.mass.max(MIN_MASS);
        let before = self.current.clone();
        for _ in 0..steps {
            // Semi-implicit Euler in value space: a = (-k*(x - target) - c*v) / m, then v += a*h, x += v*h.
            let displacement = self.current.sub(&self.target);
            let force = displacement
                .scale(-s.stiffness)
                .sub(&self.velocity.scale(s.damping));
            let accel = force.scale(1.0 / mass);
            self.velocity = self.velocity.add(&accel.scale(h));
            self.current = self.current.add(&self.velocity.scale(h));
        }
        let arrived = self.current.sub(&self.target).magnitude_sq() < DISP_EPS_SQ
            && self.velocity.magnitude_sq() < VEL_EPS_SQ;
        if arrived || self.value_is_frozen(&before) {
            return Some(self.snap_to_target());
        }
        Some(self.current.clone())
    }

    // A frame that left the value bit-identical will never move it again — same state, same forces — so the animation is over. This is what guarantees termination: the epsilons above are absolute while the value is not, so a spring on screen coordinates goes numerically dead while still short of `DISP_EPS_SQ`.
    fn value_is_frozen(&self, before: &T) -> bool {
        self.current.sub(before).magnitude_sq() == 0.0
    }

    fn plan_leg(&mut self) {
        let Curve::Tween(t) = self.curve else {
            return;
        };
        let distance = self.target.sub(&self.current).magnitude_sq().sqrt();
        let fraction = if self.settled || self.full_span <= 0.0 {
            1.0
        } else {
            (distance / self.full_span).min(1.0)
        };
        if fraction >= 1.0 {
            self.full_span = distance;
        }
        self.start = self.current.clone();
        self.elapsed_secs = 0.0;
        self.leg_secs = t.duration.as_secs_f32() * fraction;
    }

    fn wake(&mut self) {
        // Reset t0 only from rest, so an idle gap doesn't integrate as one huge step; in flight the clock keeps running.
        if self.settled {
            self.last = None;
        }
        self.settled = false;
    }

    fn snap_to_target(&mut self) -> T {
        self.current = self.target.clone();
        self.velocity = T::zero();
        self.settled = true;
        self.current.clone()
    }
}

impl<T: Lerp + 'static> Tickable for RefCell<AnimInner<T>> {
    fn tick(&self, now: Instant, scale: f32) {
        // The reactive `.set()` runs outside the RefCell borrow so an effect that re-reads or retargets this same animation cannot re-enter a live borrow.
        let (signal, value) = {
            let mut inner = self.borrow_mut();
            let value = inner.integrate(now, scale);
            (inner.signal, value)
        };
        if let Some(value) = value {
            signal.set(value);
        }
    }

    fn is_settled(&self) -> bool {
        self.borrow().settled
    }
}

/// A signal-backed value that chases a `target` over time under a [`Curve`], driven by the central ticker.
///
/// `Copy`, like every other reactive handle. The state it used to hold directly now lives in the reactive runtime's arena and this is an id into it, so a closure that reads an animation takes a register copy rather than an `Rc` bump — and the animation is disposed with the owner that built it instead of with its last handle. The ticker's `Weak` is unaffected: the `Rc` did not disappear, it changed owner.
pub struct Animated<T: Lerp + 'static> {
    state: RwSignal<Shared<T>>,
    id: u64,
    _marker: PhantomData<T>,
}

type Shared<T> = Rc<RefCell<AnimInner<T>>>;

impl<T: Lerp + 'static> Clone for Animated<T> {
    fn clone(&self) -> Self {
        *self
    }
}

// Hand-written: `#[derive(Copy)]` would demand `T: Copy`, and the parameter names what the animation carries, never what the handle stores.
impl<T: Lerp + 'static> Copy for Animated<T> {}

impl<T: Lerp + 'static> Animated<T> {
    fn inner(&self) -> Shared<T> {
        self.state.with(Rc::clone)
    }
}

impl<T: Lerp + 'static> Animated<T> {
    /// Create an animation resting at `initial`. It registers with the ticker only once retargeted away from its current goal.
    ///
    /// Belongs to whatever reactive owner is active at the call, and is freed with it — which is what you want for an animation a view builds and draws. A handle kept somewhere that outlives that owner (a store keyed by slot, a thread-local the tree does not own) wants [`Animated::detached`] instead: this one would be read after its storage was freed the first time anything retargeted it.
    pub fn new(initial: T, curve: impl Into<Curve>) -> Self {
        let signal = signal(initial.clone());
        let inner = Rc::new(RefCell::new(AnimInner {
            signal,
            current: initial.clone(),
            target: initial.clone(),
            velocity: T::zero(),
            start: initial,
            elapsed_secs: 0.0,
            leg_secs: 0.0,
            full_span: 0.0,
            curve: curve.into(),
            settled: true,
            last: None,
        }));
        Animated {
            state: reactive_core::signal(inner),
            id: ticker::next_id(),
            _marker: PhantomData,
        }
    }

    /// [`new`](Self::new), belonging to no reactive owner.
    ///
    /// For an animation whose handle lives somewhere the tree does not own — a store keyed by slot, a thread-local that outlives the widget it draws. Under [`new`](Self::new) that handle is freed with whatever scope happened to be active at the call, and the next `retarget` reads storage that is already gone; here nothing frees it but the caller dropping it from wherever it is kept, which is the lifetime such a store meant to have in the first place.
    pub fn detached(initial: T, curve: impl Into<Curve>) -> Self {
        reactive_core::detached(|| Self::new(initial, curve))
    }

    /// Aim at a new `target` (a no-op if it's already the goal); a tween interrupted mid-flight takes only the share of its duration proportional to the remaining distance, so it doesn't restart from a full leg.
    pub fn retarget(&self, target: T) {
        {
            let shared = self.inner();
            let mut inner = shared.borrow_mut();
            if target.sub(&inner.target).magnitude_sq() <= NOOP_EPS_SQ {
                return;
            }
            inner.target = target;
            inner.plan_leg();
            inner.wake();
        }
        self.register();
    }

    /// Shifts the value by `delta` and publishes it immediately, keeping the current target — for following a layout move without a visible jump. No-op under a zero time scale.
    pub fn displace(&self, delta: T) {
        if delta.magnitude_sq() <= NOOP_EPS_SQ || ticker::scale() <= 0.0 {
            return;
        }
        let (signal, value) = {
            let shared = self.inner();
            let mut inner = shared.borrow_mut();
            inner.current = inner.current.add(&delta);
            inner.plan_leg();
            inner.wake();
            (inner.signal, inner.current.clone())
        };
        signal.set(value);
        self.register();
    }

    fn register(&self) {
        // Bind the concrete Weak first so it unsize-coerces to Weak<dyn Tickable> at the call.
        let weak = Rc::downgrade(&self.inner());
        ticker::register(self.id, weak);
    }

    /// Reactive read: subscribes the calling segment to the animated value.
    pub fn get(&self) -> T {
        self.inner().borrow().signal.get()
    }

    /// A read-only handle to the underlying signal.
    pub fn read(&self) -> ReadSignal<T> {
        self.inner().borrow().signal.read_only()
    }

    /// Whether the storage behind this handle is still there — see [`reactive_core::RwSignal::is_alive`]. A `Copy` handle kept past its owner has no other way to ask.
    pub fn is_alive(&self) -> bool {
        self.state.is_alive()
    }

    /// Whether the animation has reached its target and deregistered.
    pub fn is_settled(&self) -> bool {
        self.inner().borrow().settled
    }
}

#[cfg(test)]
#[path = "animated_test.rs"]
mod tests;
