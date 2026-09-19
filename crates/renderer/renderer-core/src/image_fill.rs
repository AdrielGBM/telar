//! [`ImageFill`]: how a picture's pixels cover the rect an image command is given, and the geometry every backend reads that from.

use geometry_core::{Insets, Rect};

use crate::ImageData;

/// How a picture covers the rect of the [`DrawCommand::Image`](crate::DrawCommand::Image) that draws it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImageFill {
    /// The whole picture scaled to the rect.
    #[default]
    Stretch,
    /// Copies of the picture repeated across the rect from its top-left corner, each `scale` units per source pixel. A scale that is not a positive number draws nothing.
    Tile { scale: f32 },
    /// Nine-slice: corners kept at their size, edges stretched along their length, the middle stretched both ways.
    Slice(ImageSlice),
}

/// The four lines that cut a picture into nine pieces for [`ImageFill::Slice`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSlice {
    /// Where the cuts fall, in source pixels from each edge of the picture.
    pub insets: Insets,
    /// Units a border is drawn at per source pixel: 1 keeps the corners at their own size.
    pub scale: f32,
}

impl ImageSlice {
    pub const fn new(insets: Insets) -> Self {
        Self { insets, scale: 1.0 }
    }

    pub const fn with_scale(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// Overlapping cuts shrink in proportion and borders too large for `dest` shrink by one shared factor, as CSS `border-image` does, so a box smaller than its corners still shows whole corners.
    pub fn pieces(&self, image: (u32, u32), dest: Rect) -> impl Iterator<Item = SlicePiece> {
        let (width, height) = (image.0 as f32, image.1 as f32);
        let (left, right) = fit_cuts(self.insets.left, self.insets.right, width);
        let (top, bottom) = fit_cuts(self.insets.top, self.insets.bottom, height);
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            0.0
        };
        let shrink = [
            room(dest.width, (left + right) * scale),
            room(dest.height, (top + bottom) * scale),
        ]
        .into_iter()
        .fold(1.0, f32::min);
        let border = |cut: f32| cut * scale * shrink;
        let columns = cuts_across(
            [0.0, left, width - right, width],
            [dest.x, border(left), border(right), dest.width],
        );
        let rows = cuts_across(
            [0.0, top, height - bottom, height],
            [dest.y, border(top), border(bottom), dest.height],
        );
        (0..9).filter_map(move |index| {
            let ((sx, sw), (dx, dw)) = columns[index % 3];
            let ((sy, sh), (dy, dh)) = rows[index / 3];
            (sw > 0.0 && sh > 0.0 && dw > 0.0 && dh > 0.0).then(|| SlicePiece {
                source: Rect::new(sx, sy, sw, sh),
                dest: Rect::new(dx, dy, dw, dh),
            })
        })
    }
}

impl Default for ImageSlice {
    fn default() -> Self {
        Self::new(Insets::default())
    }
}

/// One of the nine pieces of an [`ImageSlice`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlicePiece {
    pub source: Rect,
    pub dest: Rect,
}

impl ImageFill {
    /// The size one copy of `image` covers under [`ImageFill::Tile`], or `None` when there is nothing to repeat.
    pub fn tile_size(scale: f32, image: &ImageData) -> Option<(f32, f32)> {
        (scale.is_finite() && scale > 0.0 && image.width > 0 && image.height > 0)
            .then_some((image.width as f32 * scale, image.height as f32 * scale))
    }

    /// The same fill with every length it carries in destination units multiplied by `factor`.
    pub fn scaled(self, factor: f32) -> Self {
        match self {
            ImageFill::Stretch => ImageFill::Stretch,
            ImageFill::Tile { scale } => ImageFill::Tile {
                scale: scale * factor,
            },
            ImageFill::Slice(slice) => ImageFill::Slice(slice.with_scale(slice.scale * factor)),
        }
    }
}

fn fit_cuts(near: f32, far: f32, length: f32) -> (f32, f32) {
    let (near, far) = (near.max(0.0), far.max(0.0));
    if near + far > length {
        let factor = length / (near + far);
        (near * factor, far * factor)
    } else {
        (near, far)
    }
}

fn room(available: f32, wanted: f32) -> f32 {
    if wanted > 0.0 {
        available.max(0.0) / wanted
    } else {
        1.0
    }
}

/// Source and destination spans of the three pieces along one axis, from the source cut positions and `[dest origin, near border, far border, dest length]`.
fn cuts_across(
    source: [f32; 4],
    [origin, near, far, length]: [f32; 4],
) -> [((f32, f32), (f32, f32)); 3] {
    let dest = [
        origin,
        origin + near,
        origin + length - far,
        origin + length,
    ];
    std::array::from_fn(|i| {
        (
            (source[i], source[i + 1] - source[i]),
            (dest[i], dest[i + 1] - dest[i]),
        )
    })
}

#[cfg(test)]
#[path = "image_fill_test.rs"]
mod tests;
