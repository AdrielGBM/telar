//! Comparing a snapshot with its golden: pixels within a [`Tolerance`], draw text line for line.

use serde::{Deserialize, Serialize};
use tiny_skia::{IntSize, Pixmap};

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

/// Straight, not premultiplied, RGBA8 pixels, row by row: what a PNG holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn decode(png: &[u8]) -> Result<Self, String> {
        let pixmap = Pixmap::decode_png(png).map_err(|error| error.to_string())?;
        Ok(Self::of_pixmap(&pixmap))
    }

    pub fn of_pixmap(pixmap: &Pixmap) -> Self {
        let rgba = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        Self {
            width: pixmap.width(),
            height: pixmap.height(),
            rgba,
        }
    }

    fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba[at..at + 4].try_into().ok()
    }
}

/// How two pictures differ, and a picture of where.
#[derive(Clone, Debug)]
pub(crate) struct PixelDiff {
    /// Pixels whose channels moved past the tolerance, or that only one of the two pictures has.
    pub differing: u64,
    /// The most any channel moved anywhere: 255 where a pixel is in only one picture.
    pub max_delta: u8,
    /// Every differing pixel in a loud colour over a faded copy of `actual`, as large as the larger of the two.
    pub diff: Pixmap,
}

impl PixelDiff {
    pub fn within(&self, tolerance: Tolerance) -> bool {
        self.differing <= tolerance.max_pixels
    }
}

const DIFFERING: [u8; 4] = [255, 0, 80, 255];

pub(crate) fn compare(golden: &Image, actual: &Image, tolerance: Tolerance) -> PixelDiff {
    let width = golden.width.max(actual.width).max(1);
    let height = golden.height.max(actual.height).max(1);
    let mut differing = 0u64;
    let mut max_delta = 0u8;
    let mut diff = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (delta, shown) = match (golden.pixel(x, y), actual.pixel(x, y)) {
                (Some(before), Some(after)) => (channel_delta(before, after), Some(after)),
                (_, after) => (u8::MAX, after),
            };
            max_delta = max_delta.max(delta);
            if delta > tolerance.channel {
                differing += 1;
                diff.extend_from_slice(&DIFFERING);
            } else {
                diff.extend_from_slice(&faded(shown.unwrap_or([0; 4])));
            }
        }
    }
    let size = IntSize::from_wh(width, height).expect("both sides are at least one pixel");
    PixelDiff {
        differing,
        max_delta,
        diff: Pixmap::from_vec(diff, size).expect("one opaque pixel per position"),
    }
}

fn channel_delta(before: [u8; 4], after: [u8; 4]) -> u8 {
    before
        .iter()
        .zip(after)
        .map(|(before, after)| before.abs_diff(after))
        .max()
        .unwrap_or(0)
}

/// `pixel` over white, as a light grey that keeps its shape but never competes with a differing pixel. Opaque, so it is its own premultiplied form.
fn faded([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let over_white = |channel: u8| {
        let alpha = f32::from(a) / 255.0;
        f32::from(channel) * alpha + 255.0 * (1.0 - alpha)
    };
    let luma = 0.299 * over_white(r) + 0.587 * over_white(g) + 0.114 * over_white(b);
    let grey = (255.0 - (255.0 - luma) / 4.0).round() as u8;
    [grey, grey, grey, 255]
}

/// A unified diff of `golden` against `actual`, three lines of context around each change, headed with the names of both.
pub(crate) fn line_diff(
    golden: &str,
    actual: &str,
    golden_name: &str,
    actual_name: &str,
) -> String {
    similar::TextDiff::from_lines(golden, actual)
        .unified_diff()
        .context_radius(3)
        .header(golden_name, actual_name)
        .to_string()
}

#[cfg(test)]
#[path = "compare_test.rs"]
mod tests;
