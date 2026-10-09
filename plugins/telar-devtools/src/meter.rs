//! The FPS readout's arithmetic: which frames count, over what window, and when the number shown has gone stale.

use std::collections::VecDeque;
use std::time::Duration;
use web_time::Instant;

const WINDOW: Duration = Duration::from_secs(1);

/// What the readout says: frames drawn over the last second, and the average time between them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Reading {
    pub fps: u32,
    pub frame_millis: f32,
}

/// Where the meter reads the time from: the wall clock, unless a test supplies its own.
#[derive(Clone, Copy)]
pub(crate) struct Clock(fn() -> Instant);

impl Clock {
    #[cfg(test)]
    pub fn reading(read: fn() -> Instant) -> Self {
        Self(read)
    }

    pub fn now(self) -> Instant {
        (self.0)()
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self(Instant::now)
    }
}

/// Counts the frames that carried the app's own content, so the overlay's frames and keepalive blits do not inflate the number.
#[derive(Default)]
pub(crate) struct FrameMeter {
    frames: VecDeque<Instant>,
    shown: Reading,
}

impl FrameMeter {
    /// Records a frame if it drew new content, and returns the reading as of `now`, which is then the one shown.
    pub fn sample(&mut self, now: Instant, has_content: bool) -> Reading {
        if has_content {
            self.frames.push_back(now);
        }
        let cutoff = now.checked_sub(WINDOW);
        while self
            .frames
            .front()
            .is_some_and(|at| cutoff.is_some_and(|cutoff| *at <= cutoff))
        {
            self.frames.pop_front();
        }
        self.shown = Reading {
            fps: self.frames.len() as u32,
            frame_millis: self.frame_millis(),
        };
        self.shown
    }

    /// Whether frames have aged out of the window since the last [`sample`](Self::sample), so the number shown is no longer true.
    pub fn is_stale(&self, now: Instant) -> bool {
        let cutoff = now.checked_sub(WINDOW);
        let live = self
            .frames
            .iter()
            .filter(|at| cutoff.is_none_or(|cutoff| **at > cutoff))
            .count();
        live as u32 != self.shown.fps
    }

    fn frame_millis(&self) -> f32 {
        match (self.frames.front(), self.frames.back()) {
            (Some(oldest), Some(newest)) if self.frames.len() >= 2 => {
                newest.duration_since(*oldest).as_secs_f32() * 1000.0
                    / (self.frames.len() - 1) as f32
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
#[path = "meter_test.rs"]
mod tests;
