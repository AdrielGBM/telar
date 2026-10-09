//! [`run_after`]: a callback the UI thread runs once a stretch of time has passed, which the loop sleeps until rather than polls for.
//!
//! A notice that puts itself away, a "Copied" that turns back into "Copy", a hover that opens a tooltip only once it rests: each wants to be told when a moment arrives, and nothing else. Driving that from the frame ticker keeps frames coming for the whole wait. A timer instead hands the runner a deadline: it reports how long until the earliest one with [`until_next_timer`], sleeps that long without drawing anything, and runs what came due with [`fire_timers`] on the next turn.
//!
//! Timers run on wall-clock time, not motion time: neither the motion engine's time scale nor the user's reduced-motion preference shortens one, because waiting is not animation.
//!
//! The clock is the timer service's own ([`timer_now`]): real time, moved forward by [`advance_timer_clock`] where a test or a scripted drive needs the wait to pass without waiting it out.

use std::marker::PhantomData;
use std::time::Duration;

use rustc_hash::FxHashMap;
use web_time::Instant;

use crate::leaked_cell::LeakedCell;
use crate::runtime::{SurfaceHandle, current_surface};

type TimerId = u64;

enum Clock {
    Running { due: Instant },
    Paused { left: Duration },
}

struct PendingTimer {
    clock: Clock,
    callback: Box<dyn FnOnce()>,
    surface: SurfaceHandle,
}

#[derive(Default)]
struct TimerRegistry {
    next_id: TimerId,
    pending: FxHashMap<TimerId, PendingTimer>,
    advanced: Duration,
}

impl TimerRegistry {
    fn now(&self) -> Instant {
        Instant::now() + self.advanced
    }

    fn earliest(&self) -> Option<Instant> {
        self.pending
            .values()
            .filter_map(|timer| match timer.clock {
                Clock::Running { due } => Some(due),
                Clock::Paused { .. } => None,
            })
            .min()
    }
}

thread_local! {
    static TIMERS: LeakedCell<TimerRegistry> = LeakedCell::new();
}

/// A scheduled [`run_after`]. Dropping it cancels the callback, so a timer lives exactly as long as whatever holds it.
///
/// Deliberately `!Send`: the callback it controls lives in the scheduling thread's registry.
#[must_use = "dropping a Timer cancels it"]
pub struct Timer {
    id: TimerId,
    _ui_thread_only: PhantomData<*const ()>,
}

impl Timer {
    /// Stops the wait where it is, keeping what is left of it for [`resume`](Self::resume). A paused timer never comes due, and the loop does not wake for it.
    pub fn pause(&self) {
        TIMERS.with(|t| {
            let mut registry = t.borrow_mut();
            let now = registry.now();
            if let Some(timer) = registry.pending.get_mut(&self.id)
                && let Clock::Running { due } = timer.clock
            {
                timer.clock = Clock::Paused {
                    left: due.saturating_duration_since(now),
                };
            }
        });
    }

    /// Picks the wait up again with what was left of it when it was paused. Nothing happens to one that is running or already done.
    pub fn resume(&self) {
        let is_earliest = TIMERS.with(|t| {
            let mut registry = t.borrow_mut();
            let now = registry.now();
            let Some(timer) = registry.pending.get_mut(&self.id) else {
                return false;
            };
            let Clock::Paused { left } = timer.clock else {
                return false;
            };
            let due = now + left;
            timer.clock = Clock::Running { due };
            registry.earliest() == Some(due)
        });
        if is_earliest {
            crate::task::wake_loop();
        }
    }

    /// Whether the callback has yet to run: the wait is still on, paused or not.
    pub fn is_pending(&self) -> bool {
        TIMERS.with(|t| t.borrow().pending.contains_key(&self.id))
    }

    pub fn is_paused(&self) -> bool {
        TIMERS.with(|t| {
            matches!(
                t.borrow().pending.get(&self.id),
                Some(PendingTimer {
                    clock: Clock::Paused { .. },
                    ..
                })
            )
        })
    }

    /// Drops the callback without running it. The same as dropping the timer, said out loud.
    pub fn cancel(self) {
        drop(self);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Taken out of the registry before it is dropped: the callback's captured state may schedule or cancel timers from its own `Drop`, which would re-enter this borrow.
        let cancelled = TIMERS.with(|t| t.borrow_mut().pending.remove(&self.id));
        drop(cancelled);
    }
}

/// Runs `callback` on this thread once `delay` has passed, during a later turn of the loop; see the [module docs](self).
///
/// The callback runs inside a batch and re-enters the surface that was active when it was scheduled, as a [`spawn_task`](crate::spawn_task) completion does. Keep the returned [`Timer`] for as long as the callback should still run: dropping it cancels the callback, and [`Timer::pause`] holds the wait, as a notice does while the pointer rests on it.
///
/// ```ignore
/// let reset = run_after(Duration::from_secs(2), move || copied.set(false));
/// ```
pub fn run_after(delay: Duration, callback: impl FnOnce() + 'static) -> Timer {
    let (id, is_earliest) = TIMERS.with(|t| {
        let mut registry = t.borrow_mut();
        let id = registry.next_id;
        registry.next_id += 1;
        let due = registry.now() + delay;
        let is_earliest = registry.earliest().is_none_or(|earliest| due < earliest);
        registry.pending.insert(
            id,
            PendingTimer {
                clock: Clock::Running { due },
                callback: Box::new(callback),
                surface: current_surface(),
            },
        );
        (id, is_earliest)
    });
    // A loop already asleep until a later deadline has to recompute its wait, or this one would run late.
    if is_earliest {
        crate::task::wake_loop();
    }
    Timer {
        id,
        _ui_thread_only: PhantomData,
    }
}

/// Runs the callback of every timer that has come due, earliest first. The runner calls this once per frame, on the UI thread, before `App::on_frame`.
///
/// Callbacks run inside a batch, so a frame's worth of them costs one flush. One scheduled by a callback here waits for the next call even when it is already due.
pub fn fire_timers() {
    let due = TIMERS.with(|t| {
        let registry = t.borrow();
        let now = registry.now();
        let mut due: Vec<(Instant, TimerId)> = registry
            .pending
            .iter()
            .filter_map(|(id, timer)| match timer.clock {
                Clock::Running { due } if due <= now => Some((due, *id)),
                _ => None,
            })
            .collect();
        due.sort_unstable();
        due
    });
    if due.is_empty() {
        return;
    }
    crate::batch(|| {
        for (_, id) in due {
            // Re-checked one by one, because a callback that ran before this one may have cancelled or paused it.
            let timer = TIMERS.with(|t| {
                let mut registry = t.borrow_mut();
                match registry.pending.get(&id)?.clock {
                    Clock::Running { .. } => registry.pending.remove(&id),
                    Clock::Paused { .. } => None,
                }
            });
            let Some(timer) = timer else {
                continue;
            };
            let _surface = timer.surface.enter();
            (timer.callback)();
        }
    });
}

/// How long until the earliest running timer comes due, on the timer clock: `Some(ZERO)` when one already has, `None` when none is waiting. The runner sleeps no longer than this.
pub fn until_next_timer() -> Option<Duration> {
    TIMERS.with(|t| {
        let registry = t.borrow();
        let now = registry.now();
        registry
            .earliest()
            .map(|due| due.saturating_duration_since(now))
    })
}

/// When the earliest running timer comes due, on the timer clock, or `None` when none is waiting. Unlike [`until_next_timer`], which shrinks with every read, it reads the same until the earliest deadline itself changes, so another runtime mirroring this one can tell a new deadline from an old one.
pub fn next_timer_due() -> Option<Instant> {
    TIMERS.with(|t| t.borrow().earliest())
}

/// The time timers are scheduled and come due against: real time, plus everything [`advance_timer_clock`] added.
pub fn timer_now() -> Instant {
    TIMERS.with(|t| t.borrow().now())
}

/// Moves this thread's timer clock forward by `by`, so a wait passes without being waited out. For a test or a scripted, windowless drive; nothing in a running application calls it. Call [`fire_timers`] afterwards to run what came due.
pub fn advance_timer_clock(by: Duration) {
    TIMERS.with(|t| t.borrow_mut().advanced += by);
}

/// Cancels every timer scheduled while `surface` was the active one. Use it when a surface goes away, so a wait started for it cannot write into the world it left.
pub fn cancel_timers_for(surface: SurfaceHandle) {
    let cancelled: Vec<PendingTimer> = TIMERS.with(|t| {
        let mut registry = t.borrow_mut();
        let ids: Vec<TimerId> = registry
            .pending
            .iter()
            .filter(|(_, timer)| timer.surface == surface)
            .map(|(id, _)| *id)
            .collect();
        ids.iter()
            .filter_map(|id| registry.pending.remove(id))
            .collect()
    });
    drop(cancelled);
}

/// Drops every pending callback. Called before a hot-reload dylib is closed: the callbacks are made of code that is about to be unmapped.
pub fn reset_timers() {
    let pending = TIMERS.with(|t| std::mem::take(&mut t.borrow_mut().pending));
    drop(pending);
}

#[cfg(test)]
fn pending_timer_count() -> usize {
    TIMERS.with(|t| t.borrow().pending.len())
}

#[cfg(test)]
#[path = "timer_test.rs"]
mod tests;
