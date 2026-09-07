//! [`Keyframes`]: a sequence of legs travelled in order, optionally looping or ping-ponging.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;

use reactive_core::{ReadSignal, RwSignal, signal};

use crate::easing::Easing;
use crate::lerp::Lerp;
use crate::ticker::{self, Tickable};

/// How a [`Keyframes`] sequence behaves once it reaches its last step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Repeat {
    Once,
    Loop,
    PingPong,
}

// One leg of the sequence: interpolate `start` -> `end` over `duration` under `easing`. `hold()` emits a step with start == end, so its easing is inert by construction.
struct Step<T: Lerp> {
    start: T,
    end: T,
    duration: Duration,
    easing: Easing,
}

// Which way the timeline cursor is currently moving; only meaningful for `Repeat::PingPong`.
#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Forward,
    Backward,
}

// Find the step spanning timeline position `t` (assumed clamped to `[0, total]`), returning its index and its cumulative `[start, end)` bounds.
fn locate<T: Lerp>(steps: &[Step<T>], t: f32) -> (usize, f32, f32) {
    let mut acc = 0.0;
    let last_idx = steps.len() - 1;
    for (i, step) in steps.iter().enumerate() {
        let end_cum = acc + step.duration.as_secs_f32();
        if t <= end_cum || i == last_idx {
            return (i, acc, end_cum);
        }
        acc = end_cum;
    }
    unreachable!("steps is never empty: KeyframesBuilder::start() guarantees at least one step")
}

// Pure function of timeline position: the same curve is reused for forward and backward travel, so PingPong "mirrors" a step's easing simply by re-evaluating it as `t` decreases (see Design note on Keyframes).
fn value_at<T: Lerp>(steps: &[Step<T>], t: f32) -> T {
    let (idx, start_cum, end_cum) = locate(steps, t);
    let step = &steps[idx];
    let dur = end_cum - start_cum;
    let local = if dur <= 0.0 {
        1.0
    } else {
        ((t - start_cum) / dur).clamp(0.0, 1.0)
    };
    step.start.lerp(&step.end, step.easing.apply(local))
}

pub(crate) struct KeyframesInner<T: Lerp + 'static> {
    signal: RwSignal<T>,
    steps: Vec<Step<T>>,
    total_duration: f32,
    initial: T,
    repeat: Repeat,
    direction: Direction,
    // Position along the `[0, total_duration]` timeline; for PingPong this itself moves back and forth.
    elapsed_secs: f32,
    current: T,
    // Doubles as Tickable::is_settled: true once Once completes naturally OR `stop()` was called.
    settled: bool,
    // True only when Repeat::Once ran its sequence to completion (never set by `stop()`).
    completed_once: bool,
    last: Option<Instant>,
}

impl<T: Lerp + 'static> KeyframesInner<T> {
    fn integrate(&mut self, now: Instant, scale: f32) -> Option<T> {
        if self.settled {
            return None;
        }
        if scale <= 0.0 {
            return Some(self.snap_scale_zero());
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
        // All-zero-duration sequence: Once settles instantly; Loop/PingPong just hold to avoid a `% 0.0`.
        if self.total_duration <= 0.0 {
            return matches!(self.repeat, Repeat::Once).then(|| self.finish_once());
        }
        self.advance(dt);
        if matches!(self.repeat, Repeat::Once) && self.elapsed_secs >= self.total_duration {
            return Some(self.finish_once());
        }
        self.current = value_at(&self.steps, self.elapsed_secs);
        Some(self.current.clone())
    }

    fn advance(&mut self, dt: f32) {
        match self.repeat {
            Repeat::Once => self.elapsed_secs = (self.elapsed_secs + dt).min(self.total_duration),
            // Wrapping back to 0 replays the first step's start value, which is a deliberate discrete jump if it differs from the last step's end (CSS-style restart, not a smoothed loop).
            Repeat::Loop => self.elapsed_secs = (self.elapsed_secs + dt) % self.total_duration,
            Repeat::PingPong => {
                let mut remaining = dt;
                while remaining > 0.0 {
                    match self.direction {
                        Direction::Forward => {
                            let to_edge = self.total_duration - self.elapsed_secs;
                            if remaining < to_edge {
                                self.elapsed_secs += remaining;
                                remaining = 0.0;
                            } else {
                                self.elapsed_secs = self.total_duration;
                                remaining -= to_edge;
                                self.direction = Direction::Backward;
                            }
                        }
                        Direction::Backward => {
                            let to_edge = self.elapsed_secs;
                            if remaining < to_edge {
                                self.elapsed_secs -= remaining;
                                remaining = 0.0;
                            } else {
                                self.elapsed_secs = 0.0;
                                remaining -= to_edge;
                                self.direction = Direction::Forward;
                            }
                        }
                    }
                }
            }
        }
    }

    fn finish_once(&mut self) -> T {
        self.elapsed_secs = self.total_duration;
        self.current = self.steps.last().expect("steps is never empty").end.clone();
        self.completed_once = true;
        self.settled = true;
        self.current.clone()
    }

    // Reduced-motion "instant": `Once` jumps to the sequence end, `Loop`/`PingPong` to the end of whichever step is in flight — coherent with `Animated`'s snap-to-target without freezing an indefinite repeat.
    fn snap_scale_zero(&mut self) -> T {
        if matches!(self.repeat, Repeat::Once) {
            return self.finish_once();
        }
        let (_, start_cum, end_cum) = locate(&self.steps, self.elapsed_secs);
        self.elapsed_secs = match self.direction {
            Direction::Forward => end_cum,
            Direction::Backward => start_cum,
        };
        self.current = value_at(&self.steps, self.elapsed_secs);
        self.current.clone()
    }
}

impl<T: Lerp + 'static> Tickable for RefCell<KeyframesInner<T>> {
    fn tick(&self, now: Instant, scale: f32) {
        // As in Animated: `.set()` runs outside the borrow so a re-entrant read/control call cannot hit a live borrow.
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

/// A signal-backed, autonomous multi-step animation: it plays a fixed sequence rather than chasing a live target, driven by the same central ticker as [`crate::Animated`].
pub struct Keyframes<T: Lerp + 'static> {
    inner: Rc<RefCell<KeyframesInner<T>>>,
    id: u64,
}

impl<T: Lerp + 'static> Clone for Keyframes<T> {
    fn clone(&self) -> Self {
        Keyframes {
            inner: Rc::clone(&self.inner),
            id: self.id,
        }
    }
}

impl<T: Lerp + 'static> Keyframes<T> {
    /// Start building a sequence resting at `initial`.
    // Returns a builder rather than Self by design (entry point of the builder chain, cf. Animated::new).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(initial: T) -> KeyframesBuilder<T> {
        KeyframesBuilder {
            initial: initial.clone(),
            cursor: initial,
            steps: Vec::new(),
        }
    }

    /// Reactive read: subscribes the calling segment to the current value.
    pub fn get(&self) -> T {
        self.inner.borrow().signal.get()
    }

    /// A read-only handle to the underlying signal.
    pub fn read(&self) -> ReadSignal<T> {
        self.inner.borrow().signal.read_only()
    }

    /// Rewind to t=0 and (re)register with the ticker, whatever the current state.
    pub fn restart(&self) {
        // The set happens outside the borrow, like tick(): at batch depth 0 it flushes synchronously and a subscribed segment's re-entrant get() needs the RefCell.
        let (signal, initial) = {
            let mut inner = self.inner.borrow_mut();
            inner.elapsed_secs = 0.0;
            inner.direction = Direction::Forward;
            inner.current = inner.initial.clone();
            inner.settled = false;
            inner.completed_once = false;
            // Re-establish t0 on the next tick, same reasoning as Animated::retarget.
            inner.last = None;
            (inner.signal, inner.current.clone())
        };
        // Registration is idempotent (keyed by id), so re-registering an already-active sequence is harmless. Register before the set so a re-entrant has_active() during the flush already sees it active.
        let weak = Rc::downgrade(&self.inner);
        ticker::register(self.id, weak);
        signal.set(initial);
    }

    /// Stop advancing and freeze at the current value; deregisters from the ticker.
    pub fn stop(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.settled = true;
        inner.last = None;
    }

    /// True only once a `Repeat::Once` sequence has played through to its last step.
    pub fn is_finished(&self) -> bool {
        self.inner.borrow().completed_once
    }
}

/// Accumulates steps for a [`Keyframes`] sequence before it starts.
pub struct KeyframesBuilder<T: Lerp + 'static> {
    initial: T,
    // Running end value of the last appended step (or `initial` if none yet), so the next step knows its start.
    cursor: T,
    steps: Vec<Step<T>>,
}

impl<T: Lerp + 'static> KeyframesBuilder<T> {
    /// Append a step interpolating from the current end of the sequence to `value`.
    pub fn then(mut self, value: T, duration: Duration, easing: Easing) -> Self {
        self.steps.push(Step {
            start: self.cursor,
            end: value.clone(),
            duration,
            easing,
        });
        self.cursor = value;
        self
    }

    /// Append a step that holds the current value for `duration` (delay / stagger).
    pub fn hold(mut self, duration: Duration) -> Self {
        self.steps.push(Step {
            start: self.cursor.clone(),
            end: self.cursor.clone(),
            duration,
            easing: Easing::Linear,
        });
        self
    }

    /// Register the sequence with the ticker and start playback under `repeat`.
    pub fn start(self, repeat: Repeat) -> Keyframes<T> {
        let steps = if self.steps.is_empty() {
            // A sequence needs at least one step so `locate`/`value_at` never see an empty slice.
            vec![Step {
                start: self.initial.clone(),
                end: self.initial.clone(),
                duration: Duration::ZERO,
                easing: Easing::Linear,
            }]
        } else {
            self.steps
        };
        let total_duration = steps.iter().map(|s| s.duration.as_secs_f32()).sum();
        let signal = signal(self.initial.clone());
        let inner = Rc::new(RefCell::new(KeyframesInner {
            signal,
            steps,
            total_duration,
            initial: self.initial.clone(),
            repeat,
            direction: Direction::Forward,
            elapsed_secs: 0.0,
            current: self.initial,
            settled: false,
            completed_once: false,
            last: None,
        }));
        let kf = Keyframes {
            inner,
            id: ticker::next_id(),
        };
        let weak = Rc::downgrade(&kf.inner);
        ticker::register(kf.id, weak);
        kf
    }
}

#[cfg(test)]
#[path = "keyframes_test.rs"]
mod tests;
