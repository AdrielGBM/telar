//! A reactive elapsed time that runs only while something reads it.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use reactive_core::{Memo, ReadSignal, RwSignal, memo, on_cleanup, signal};
use web_time::Instant;

use crate::ticker::{self, Tickable};

/// The longest stretch of one frame that counts as elapsed time. A surface that was hidden, suspended or stalled delivers its next frame long after the last one, and a clock that integrated that gap would jump by the whole time away.
pub const MAX_FRAME_STEP: Duration = Duration::from_millis(100);

struct FrameClockInner {
    signal: RwSignal<Duration>,
    elapsed: Duration,
    last: Option<Instant>,
    paused: bool,
}

impl FrameClockInner {
    fn integrate(&mut self, now: Instant, scale: f32) -> Option<Duration> {
        if scale <= 0.0 {
            self.paused = true;
            self.last = None;
            return None;
        }
        self.paused = false;
        if !self.signal.has_subscribers() {
            self.last = None;
            return None;
        }
        let last = self.last.replace(now)?;
        let step = now.saturating_duration_since(last).min(MAX_FRAME_STEP);
        let step =
            Duration::try_from_secs_f64(step.as_secs_f64() * f64::from(scale)).unwrap_or_default();
        if step.is_zero() {
            return None;
        }
        self.elapsed += step;
        Some(self.elapsed)
    }
}

impl Tickable for RefCell<FrameClockInner> {
    fn tick(&self, now: Instant, scale: f32) {
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
        let inner = self.borrow();
        inner.paused || !inner.signal.has_subscribers()
    }

    fn resumable(&self) -> bool {
        true
    }
}

/// Time elapsed since the clock was made, counting only the frames it actually ran, as a signal.
///
/// It exists for motion that is a function of time rather than a transition toward a target: an idle sway, a shimmer, a procedural drift. It is driven by the same ticker as [`crate::Animated`] and [`crate::Keyframes`], so it costs frames only while it runs.
///
/// **It runs only while a reactive reader is subscribed.** An effect, memo or view segment that calls [`get`](Self::get) is a reader; a read outside any of those, or by something that stopped reading on its last run, is not. With no reader the clock holds its value and requests no frames, so a scene that stops reading it goes back to sleep on its own. Gate the read to scope the clock: `if moving.get() { clock.get() } else { clock.peek() }`.
///
/// **Units.** [`get`](Self::get) is a [`Duration`] on a monotonic clock; [`millis`](Self::millis) and [`secs`](Self::secs) are the same reading as `f32`. Zero is the moment the clock was made, and only time it ran counts: stretches with no reader, reduced motion, or a surface that was not delivering frames are not added, so the reading is continuous across them and resumes where it stopped. A single frame contributes at most [`MAX_FRAME_STEP`], which is what keeps a hidden browser tab or a suspended window from adding the time it was away.
///
/// **Reduced motion and time scale.** Each frame's step is multiplied by the ticker's time scale, so [`crate::set_scale`] slows it. When the user prefers less motion (see [`crate::follow_reduced_motion`]) the scale is zero and the clock freezes at its current reading and requests no frames, as a looping [`crate::Keyframes`] stops. It does not jump to an end, because it has none. Turning the preference off resumes it from the same reading.
///
/// A clone is another handle on the same clock. The clock deregisters when the last handle drops.
#[derive(Clone)]
pub struct FrameClock {
    inner: Rc<RefCell<FrameClockInner>>,
    signal: RwSignal<Duration>,
}

impl FrameClock {
    /// A clock at zero, registered with the ticker and idle until something reads it.
    pub fn new() -> Self {
        let signal = signal(Duration::ZERO);
        let inner = Rc::new(RefCell::new(FrameClockInner {
            signal,
            elapsed: Duration::ZERO,
            last: None,
            paused: false,
        }));
        let weak = Rc::downgrade(&inner);
        ticker::register(ticker::next_id(), weak);
        Self { inner, signal }
    }

    /// Reactive read: subscribes the calling segment, which is what keeps the clock running.
    pub fn get(&self) -> Duration {
        self.signal.get()
    }

    /// The reading without subscribing the caller, so it neither wakes the caller nor keeps the clock running.
    pub fn peek(&self) -> Duration {
        self.signal.peek()
    }

    /// [`get`](Self::get) in milliseconds.
    pub fn millis(&self) -> f32 {
        self.get().as_secs_f32() * 1000.0
    }

    /// [`get`](Self::get) in seconds.
    pub fn secs(&self) -> f32 {
        self.get().as_secs_f32()
    }

    /// A read-only handle on the underlying signal. Reading it subscribes like [`get`](Self::get).
    pub fn read(&self) -> ReadSignal<Duration> {
        self.signal.read_only()
    }

    /// Whether a reader is subscribed right now, which is when the clock asks for frames.
    pub fn has_readers(&self) -> bool {
        self.signal.has_subscribers()
    }

    /// Back to zero. The next frame it runs establishes the new origin.
    pub fn reset(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.elapsed = Duration::ZERO;
            inner.last = None;
        }
        self.signal.set(Duration::ZERO);
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new()
    }
}

/// A frame clock that lives as long as the scope calling this, read as a signal.
///
/// Time since this was called; see [`FrameClock`] for units, reduced motion and when it runs. Call it in a component's setup and read the result where the time is used.
pub fn use_frame_time() -> ReadSignal<Duration> {
    let clock = FrameClock::new();
    let read = clock.read();
    on_cleanup(move || drop(clock));
    read
}

/// [`use_frame_time`] that reads the clock only while `running` is true.
///
/// While `running` is false the memo holds the last reading and subscribes to nothing on the clock, so no frames are asked for. `running` may read signals; the memo follows them. The reading continues from where it stopped when `running` turns true again.
pub fn use_frame_time_while(running: impl Fn() -> bool + 'static) -> Memo<Duration> {
    let time = use_frame_time();
    memo(move || if running() { time.get() } else { time.peek() })
}

#[cfg(test)]
#[path = "frame_clock_test.rs"]
mod tests;
