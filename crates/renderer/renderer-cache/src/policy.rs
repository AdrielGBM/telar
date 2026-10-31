//! Admission: what is worth keeping, so a string minted once per second never evicts a glyph in use.

use std::time::Duration;

/// What bounds a [`Cache`](crate::Cache): how much it may hold, when it must let go, and what it takes to get in.
///
/// The three are separate because a budget alone answers only the first. A cache that never fills its budget never evicts, so its coldest entry outlives every process that reads it; and a cache with no admission rule stores keys that by construction repeat zero times — a clock at `%H:%M:%S`, a byte counter, a download percentage. Bounding by more than size is the usual answer for exactly this reason: evicting by frame age, purging what has gone unused for a while, tying the budget to the surface.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// The ceiling, in bytes.
    ///
    /// Bytes for every cache, including the ones that used to cap an entry count instead. Two units meant two kinds of bound to reason about and no way to compare a 1000-entry cache against a 32 MB one; measuring a cache of 8-byte values in bytes costs nothing and leaves one number to read.
    pub capacity: usize,
    /// How long an entry survives with nothing asking for it. `None` bounds by [`capacity`](Self::capacity) alone.
    pub idle: Option<Duration>,
    /// Whether a value has to be offered twice before it is kept. Earns its keep only where the key space is unbounded *and* the entries are large; where keys are stable — an icon, a shadow's geometry — it would almost never reject, and would buy a guaranteed first-frame miss for nothing.
    pub admit_on_second_use: bool,
}

impl Policy {
    /// A policy bounded by `capacity` bytes alone: nothing expires and every offered value is admitted.
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            idle: None,
            admit_on_second_use: false,
        }
    }

    /// Lets an entry go once nothing has asked for it for `after`.
    pub const fn idle(mut self, after: Duration) -> Self {
        self.idle = Some(after);
        self
    }

    /// Keeps a value only once it has been offered twice, so a key seen a single time never displaces an entry in use.
    pub const fn admit_on_second_use(mut self) -> Self {
        self.admit_on_second_use = true;
        self
    }
}
