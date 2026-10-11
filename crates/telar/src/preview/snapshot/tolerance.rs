//! [`Tolerance`]: how far a snapshot's picture may drift from its golden and still match it.

use serde::{Deserialize, Serialize};

/// How far a picture may drift from its golden and still match it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tolerance {
    /// The most one channel of a pixel, on the 0–255 scale, may move before the pixel counts as differing: room for a rasteriser's rounding, which is never a change anyone drew.
    pub channel: u8,
    /// How many pixels may differ and the picture still match.
    pub max_pixels: u64,
}

impl Tolerance {
    pub const fn new(channel: u8, max_pixels: u64) -> Self {
        Self {
            channel,
            max_pixels,
        }
    }
}

impl Default for Tolerance {
    /// Two steps a channel and no pixel past them: the same rasteriser and the same faces draw the same picture, so anything more is a change.
    fn default() -> Self {
        Self::new(2, 0)
    }
}
