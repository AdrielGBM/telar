use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

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
        self.last = Some(now);
        let dt = now.saturating_duration_since(last).as_secs_f32() * scale;
        if dt <= 0.0 {
            return None;
        }
        match self.curve {
            Curve::Tween(t) => self.step_tween(t, dt),
            Curve::Spring(s) => self.step_spring(s, dt),
        }
    }

    fn step_tween(&mut self, t: Tween, dt: f32) -> Option<T> {
        self.elapsed_secs += dt;
        let duration = t.duration.as_secs_f32();
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
        let settled = self.current.sub(&self.target).magnitude_sq() < DISP_EPS_SQ
            && self.velocity.magnitude_sq() < VEL_EPS_SQ;
        if settled {
            return Some(self.snap_to_target());
        }
        Some(self.current.clone())
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
            (inner.signal.clone(), value)
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
pub struct Animated<T: Lerp + 'static> {
    inner: Rc<RefCell<AnimInner<T>>>,
    id: u64,
}

impl<T: Lerp + 'static> Clone for Animated<T> {
    fn clone(&self) -> Self {
        Animated {
            inner: Rc::clone(&self.inner),
            id: self.id,
        }
    }
}

impl<T: Lerp + 'static> Animated<T> {
    /// Create an animation resting at `initial`. It registers with the ticker only once retargeted away from its current goal.
    pub fn new(initial: T, curve: impl Into<Curve>) -> Self {
        let signal = signal(initial.clone());
        let inner = Rc::new(RefCell::new(AnimInner {
            signal,
            current: initial.clone(),
            target: initial.clone(),
            velocity: T::zero(),
            start: initial,
            elapsed_secs: 0.0,
            curve: curve.into(),
            settled: true,
            last: None,
        }));
        Animated {
            inner,
            id: ticker::next_id(),
        }
    }

    /// Aim at a new `target`. Springs keep position and velocity (interruptible); tweens restart from the current value over the full duration. Retargeting to the current goal is a no-op.
    pub fn retarget(&self, target: T) {
        {
            let mut inner = self.inner.borrow_mut();
            if target.sub(&inner.target).magnitude_sq() <= NOOP_EPS_SQ {
                return;
            }
            match inner.curve {
                Curve::Spring(_) => {
                    inner.target = target;
                }
                Curve::Tween(_) => {
                    inner.start = inner.current.clone();
                    inner.elapsed_secs = 0.0;
                    inner.target = target;
                }
            }
            inner.settled = false;
            // Re-establish t0 on the next tick so a gap since the last activity does not integrate as one huge step.
            inner.last = None;
        }
        // Bind the concrete Weak first so it unsize-coerces to Weak<dyn Tickable> at the call.
        let weak = Rc::downgrade(&self.inner);
        ticker::register(self.id, weak);
    }

    /// Reactive read: subscribes the calling segment to the animated value.
    pub fn get(&self) -> T {
        self.inner.borrow().signal.get()
    }

    /// A read-only handle to the underlying signal.
    pub fn read(&self) -> ReadSignal<T> {
        self.inner.borrow().signal.read_only()
    }

    /// Whether the animation has reached its target and deregistered.
    pub fn is_settled(&self) -> bool {
        self.inner.borrow().settled
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::curve::{spring, tween};
    use crate::easing::Easing;
    use crate::ticker::{has_active, reset, set_scale, tick};

    // Each test isolates the thread-local ticker state; libtest runs one thread per test but this is defensive against thread reuse.
    fn fresh() -> Instant {
        reset();
        set_scale(1.0);
        Instant::now()
    }

    #[test]
    fn tween_reaches_eased_value_at_half_duration() {
        let base = fresh();
        let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::EaseInOut));
        a.retarget(1.0);
        tick(base);
        tick(base + Duration::from_millis(100));
        let expected = 0.0f32.lerp(&1.0, Easing::EaseInOut.apply(0.5));
        assert!(
            (a.get() - expected).abs() < 1e-4,
            "{} != {expected}",
            a.get()
        );
    }

    #[test]
    fn tween_settles_at_target_and_goes_inactive() {
        let base = fresh();
        let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
        a.retarget(1.0);
        tick(base);
        tick(base + Duration::from_millis(200));
        assert!((a.get() - 1.0).abs() < 1e-6);
        assert!(a.is_settled());
        assert!(!has_active());
    }

    #[test]
    fn first_tick_does_not_move_the_value() {
        let base = fresh();
        let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
        a.retarget(1.0);
        // The establishing tick only records t0; nothing is set yet.
        tick(base);
        assert_eq!(a.get(), 0.0);
        assert!(has_active());
    }

    #[test]
    fn spring_settles_at_target() {
        let base = fresh();
        let a = Animated::new(0.0f32, spring(120.0, 22.0));
        a.retarget(1.0);
        tick(base);
        let mut now = base;
        for _ in 0..1000 {
            now += Duration::from_millis(16);
            tick(now);
            if !has_active() {
                break;
            }
        }
        assert!(!has_active(), "spring never settled");
        assert!((a.get() - 1.0).abs() < 1e-2, "settled at {}", a.get());
    }

    #[test]
    fn spring_preserves_velocity_across_retarget() {
        let base = fresh();
        // Underdamped so it builds clear upward velocity early in the flight.
        let a = Animated::new(0.0f32, spring(120.0, 14.0));
        a.retarget(1.0);
        tick(base);
        tick(base + Duration::from_millis(16));
        tick(base + Duration::from_millis(32));
        let value_before = a.get();
        // Retarget to the current value: with zero displacement, only preserved velocity can still move it.
        a.retarget(value_before);
        tick(base + Duration::from_millis(33));
        tick(base + Duration::from_millis(37));
        assert!(
            a.get() > value_before,
            "momentum lost: {} !> {value_before}",
            a.get()
        );
    }

    #[test]
    fn retarget_to_current_goal_is_a_noop() {
        let _ = fresh();
        let a = Animated::new(5.0f32, tween(Duration::from_millis(200), Easing::Linear));
        a.retarget(5.0);
        assert!(a.is_settled());
        assert!(!has_active());
    }

    #[test]
    fn same_target_retarget_does_not_restart_the_tween() {
        let base = fresh();
        let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
        a.retarget(1.0);
        tick(base);
        tick(base + Duration::from_millis(100));
        assert!((a.get() - 0.5).abs() < 1e-4);
        // A no-op retarget must not reset the timeline; progress continues.
        a.retarget(1.0);
        tick(base + Duration::from_millis(150));
        assert!((a.get() - 0.75).abs() < 1e-4, "restarted: {}", a.get());
    }

    #[test]
    fn scale_zero_jumps_straight_to_target() {
        let base = fresh();
        set_scale(0.0);
        let a = Animated::new(0.0f32, spring(120.0, 14.0));
        a.retarget(1.0);
        tick(base);
        assert_eq!(a.get(), 1.0);
        assert!(a.is_settled());
        assert!(!has_active());
        set_scale(1.0);
    }

    #[test]
    fn dropped_animation_deregisters() {
        let base = fresh();
        let a = Animated::new(0.0f32, tween(Duration::from_millis(200), Easing::Linear));
        a.retarget(1.0);
        tick(base);
        assert!(has_active());
        drop(a);
        assert!(!has_active());
    }
}
