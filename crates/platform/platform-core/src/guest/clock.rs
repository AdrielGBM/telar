//! Time across the boundary: a [`Timestamp`] counted from an [`Epoch`] the host sets when it loads a guest, since an `Instant` means nothing outside the std that made it.
//!
//! Both copies of std read the same monotonic clock of the OS, so an instant lies the same distance from a shared moment on either side whatever each copy's `Instant` looks like inside. The host reads its epoch and hands the guest its own reading of the clock against it; the guest takes its epoch as that far back from its own now. The two epochs differ by the length of that one call, a constant, so the time between two timestamps is exact on both sides and nothing drifts.

use std::time::{Duration, Instant};

/// An instant as signed nanoseconds since the [`Epoch`] both sides agreed on: earlier than it is negative, and ±292 years either way is representable.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    pub nanos: i64,
}

/// One side's reading of the moment timestamps count from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch(Instant);

impl Epoch {
    /// The host's, read when it loads a guest; it hands the guest [`now`](Self::now) for the guest to [`adopt`](Self::adopt).
    pub fn start() -> Self {
        Self(Instant::now())
    }

    /// The guest's: the epoch that puts this side's now at `now` on the host's.
    pub fn adopt(now: Timestamp) -> Self {
        Self(shift(Instant::now(), -i128::from(now.nanos)))
    }

    pub fn now(self) -> Timestamp {
        self.stamp(Instant::now())
    }

    pub fn stamp(self, instant: Instant) -> Timestamp {
        let nanos = match instant.checked_duration_since(self.0) {
            Some(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
            None => -i128::try_from(self.0.duration_since(instant).as_nanos()).unwrap_or(i128::MAX),
        };
        Timestamp {
            nanos: nanos.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
        }
    }

    pub fn instant(self, stamp: Timestamp) -> Instant {
        shift(self.0, i128::from(stamp.nanos))
    }
}

// An instant the platform's clock cannot represent stays where it was, which only a timestamp centuries away from the epoch reaches.
fn shift(instant: Instant, nanos: i128) -> Instant {
    let by = Duration::from_nanos(nanos.unsigned_abs().min(u128::from(u64::MAX)) as u64);
    let shifted = if nanos >= 0 {
        instant.checked_add(by)
    } else {
        instant.checked_sub(by)
    };
    shifted.unwrap_or(instant)
}

#[cfg(test)]
#[path = "clock_test.rs"]
mod tests;
