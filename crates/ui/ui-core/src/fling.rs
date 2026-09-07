//! What a scroll keeps doing after the finger lets go.
//!
//! A drag that ends while still moving does not stop where it was released: it carries on and slows down, and every platform's scroll does this because a list that stops dead reads as one that caught on something. Telar's own scroll had none — measured on a phone, three offsets over thirty-four milliseconds and then nothing.
//!
//! Not an [`Animated`](motion_core::Animated), and the difference is the point: an animation eases a value between two endpoints it knows in advance. A fling has no endpoint. It has a velocity, a decay, and a bound it may or may not reach — where it stops is an outcome, not an input. So it integrates itself against the same frame clock animations use, and stops when it runs out or hits the end of the content.

use std::cell::Cell;
use std::rc::Rc;

use reactive_core::RwSignal;
use web_time::Instant;

/// How fast a gesture must still be going, in logical pixels a second, for letting go to mean anything.
///
/// Below it a release is somebody stopping deliberately, and carrying on would be the list disobeying.
const MIN_VELOCITY: f32 = 90.0;

/// Fraction of the velocity left after one second.
///
/// `0.998` per millisecond, which is what the platforms settled on and comes to this over a second. A decay rather than a duration, so a hard flick travels further than a gentle one without either being timed — and measured on a phone: at `0.002` a fling covered a hundred and fifty pixels and was over in a tenth of a second, which reads as the list snagging rather than gliding.
const RETAINED_PER_SECOND: f32 = 0.135;

/// Below this a frame moves the content by well under a pixel, which is a stop.
const STOP_VELOCITY: f32 = 40.0;

/// The velocity a gesture leaves behind, as a rolling estimate.
///
/// Rolling rather than the last delta alone: a finger lifting often reports one short move as it goes, and a fling built from that one sample stops almost immediately no matter how fast the gesture was.
#[derive(Default)]
pub(crate) struct Velocity {
    estimate: f32,
    last: Option<Instant>,
}

impl Velocity {
    /// Folds one movement into the estimate. `delta` is the offset change, in logical pixels.
    pub(crate) fn record(&mut self, delta: f32) {
        let now = Instant::now();
        let seconds = self
            .last
            .replace(now)
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or_default();
        // A gap means the gesture paused, and a pause is a stop.
        if seconds <= 0.0 || seconds > 0.1 {
            self.estimate = 0.0;
            return;
        }
        let sample = delta / seconds;
        // Weighted towards what just happened, because that is what the hand last did.
        self.estimate = self.estimate * 0.3 + sample * 0.7;
    }

    pub(crate) fn take(&mut self) -> f32 {
        self.last = None;
        std::mem::take(&mut self.estimate)
    }

    pub(crate) fn clear(&mut self) {
        self.estimate = 0.0;
        self.last = None;
    }
}

/// One scroll offset carrying on under its own momentum.
pub(crate) struct Fling {
    offset: RwSignal<f32>,
    /// Where it may travel between, read once when the fling starts: the content does not resize under a gesture nobody is making.
    bounds: (f32, f32),
    velocity: Cell<f32>,
    last: Cell<Option<Instant>>,
    stopped: Cell<bool>,
}

impl Fling {
    /// Starts `offset` moving at `velocity` logical pixels a second, within `bounds`.
    ///
    /// `None` where there is nothing to carry: too slow to mean anything, or already against the edge it is heading for.
    pub(crate) fn start(
        offset: RwSignal<f32>,
        velocity: f32,
        bounds: (f32, f32),
    ) -> Option<Rc<Self>> {
        if velocity.abs() < MIN_VELOCITY || bounds.1 <= bounds.0 {
            return None;
        }
        let at = offset.peek();
        if (velocity < 0.0 && at <= bounds.0) || (velocity > 0.0 && at >= bounds.1) {
            return None;
        }
        let fling = Rc::new(Self {
            offset,
            bounds,
            velocity: Cell::new(velocity),
            last: Cell::new(None),
            stopped: Cell::new(false),
        });
        let id = motion_core::next_id();
        motion_core::register(
            id,
            Rc::downgrade(&fling) as std::rc::Weak<dyn motion_core::Tickable>,
        );
        Some(fling)
    }

    /// Ends it where it stands. What stops a fling is a hand on the screen, and the offset it had reached is the offset it keeps.
    pub(crate) fn stop(&self) {
        self.stopped.set(true);
    }
}

impl motion_core::Tickable for Fling {
    fn tick(&self, now: Instant, scale: f32) {
        if self.stopped.get() {
            return;
        }
        let Some(last) = self.last.replace(Some(now)) else {
            // The first tick only establishes when "now" is; there is no elapsed time to integrate yet.
            return;
        };
        let seconds = now.duration_since(last).as_secs_f32() * scale;
        if seconds <= 0.0 {
            return;
        }
        let velocity = self.velocity.get();
        let moved = self.offset.peek() + velocity * seconds;
        let clamped = moved.clamp(self.bounds.0, self.bounds.1);
        self.offset.set(clamped);
        // Meeting the end stops it, rather than pressing on against a bound it cannot pass.
        if clamped != moved {
            self.stopped.set(true);
            return;
        }
        let remaining = velocity * RETAINED_PER_SECOND.powf(seconds);
        self.velocity.set(remaining);
        if remaining.abs() < STOP_VELOCITY {
            self.stopped.set(true);
        }
    }

    fn is_settled(&self) -> bool {
        self.stopped.get()
    }
}

#[cfg(test)]
#[path = "fling_test.rs"]
mod tests;

/// How much of the distance left is still uncovered after one second.
///
/// A notch lands in about a tenth of a second, which is roughly what a browser takes and is short enough that the movement reads as the wheel's own rather than as the list lagging behind it.
const GLIDE_REMAINING_PER_SECOND: f32 = 1e-9;

/// Under half a pixel from the target is arrived.
const GLIDE_EPSILON: f32 = 0.5;

/// One scroll offset easing towards where a wheel asked it to be.
///
/// The opposite problem to a [`Fling`], and the reason they are different types rather than one: a notch says exactly how far to go and nothing about how fast, so this knows its destination from the start and a fling never does. What it borrows is the frame clock — a wheel that jumps its whole notch in one frame gives no sense of which way the content went, which is why every desktop platform animates the gap.
pub(crate) struct Glide {
    offset: RwSignal<f32>,
    target: Cell<f32>,
    last: Cell<Option<Instant>>,
    stopped: Cell<bool>,
}

impl Glide {
    /// Eases `offset` to `target`, clamped into `bounds`.
    pub(crate) fn start(
        offset: RwSignal<f32>,
        target: f32,
        bounds: (f32, f32),
    ) -> Option<Rc<Self>> {
        let target = target.clamp(bounds.0, bounds.1);
        if (target - offset.peek()).abs() < GLIDE_EPSILON {
            return None;
        }
        let glide = Rc::new(Self {
            offset,
            target: Cell::new(target),
            last: Cell::new(None),
            stopped: Cell::new(false),
        });
        motion_core::register(
            motion_core::next_id(),
            Rc::downgrade(&glide) as std::rc::Weak<dyn motion_core::Tickable>,
        );
        Some(glide)
    }

    /// Adds another notch to what this one is already covering, and says whether it could take it.
    ///
    /// Turning the wheel again mid-glide means *further*, not *instead*: restarting from where the content happens to have reached loses the ground the first notch had not covered yet, so a fast series of notches would travel less than the same notches turned slowly.
    pub(crate) fn extend(&self, delta: f32, bounds: (f32, f32)) -> bool {
        if self.stopped.get() {
            return false;
        }
        self.target
            .set((self.target.get() + delta).clamp(bounds.0, bounds.1));
        true
    }

    /// Ends it where it stands, for a gesture or a jump that is now in charge of this offset.
    pub(crate) fn stop(&self) {
        self.stopped.set(true);
    }
}

impl motion_core::Tickable for Glide {
    fn tick(&self, now: Instant, scale: f32) {
        if self.stopped.get() {
            return;
        }
        // The first tick only establishes when "now" is; there is no elapsed time to ease over yet.
        let Some(last) = self.last.replace(Some(now)) else {
            return;
        };
        let seconds = now.duration_since(last).as_secs_f32() * scale;
        if seconds <= 0.0 {
            return;
        }
        let target = self.target.get();
        let at = self.offset.peek();
        let remaining = (target - at) * GLIDE_REMAINING_PER_SECOND.powf(seconds);
        if remaining.abs() < GLIDE_EPSILON {
            self.offset.set(target);
            self.stopped.set(true);
            return;
        }
        self.offset.set(target - remaining);
    }

    fn is_settled(&self) -> bool {
        self.stopped.get()
    }
}

#[cfg(test)]
#[path = "fling_glide_test.rs"]
mod glide_tests;
