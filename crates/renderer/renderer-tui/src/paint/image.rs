//! A picture in the only resolution a terminal has.

use geometry_core::Rect;
use renderer_core::{Color, ImageData, Raster};

use super::Painter;
use crate::cell::{Attrs, Grapheme};

/// The upper half block, which is what lets one cell show two colours: its foreground paints the top half and its background the bottom.
///
/// A cell is twice as tall as it is wide, so splitting it in two is also what makes a sample square — the picture keeps its proportions instead of coming out squat. Unicode has quadrants and sextants that would divide it further, but every one of them still leaves a cell two colours, so the extra divisions buy detail only where the extra pieces happen to agree. Halves are the division every terminal font has.
const UPPER_HALF: char = '▀';

/// How many source pixels one half-cell reads along each axis at most. See [`stride`].
const SAMPLES_PER_HALF_CELL: u32 = 4;

impl Painter<'_> {
    /// Draws `data` into `rect` as coloured half-cells.
    pub(crate) fn image(&mut self, data: &ImageData, rect: Rect, raster: Raster) {
        let Some(source) = Source::of(data) else {
            return;
        };
        let placed = renderer_core::transform_clip_rect(self.matrix(), rect);
        if placed.width <= 0.0 || placed.height <= 0.0 {
            return;
        }
        let cells = self.cells_of(rect);
        let (cw, ch) = (self.cell.width, self.cell.height);
        for row in cells.row0..cells.row1 {
            for col in cells.col0..cells.col1 {
                let x = (col as f32 * cw, (col + 1) as f32 * cw);
                let y = row as f32 * ch;
                let top = source.sample(&placed, raster, x, (y, y + ch / 2.0));
                let bottom = source.sample(&placed, raster, x, (y + ch / 2.0, y + ch));
                self.half_cell(col, row, top, bottom);
            }
        }
    }

    /// Paints one cell's two samples, leaving it untouched where the picture covers nothing.
    ///
    /// Untouched rather than cleared: a picture is overwhelmingly drawn over something — an icon on a button, a logo on a panel — and the transparent margin around it is not the picture asking for that background back.
    fn half_cell(&mut self, col: i32, row: i32, top: Color, bottom: Color) {
        let (top, bottom) = (self.faded(top), self.faded(bottom));
        if top.a <= 0.0 && bottom.a <= 0.0 {
            return;
        }
        let (Ok(c), Ok(r)) = (u16::try_from(col), u16::try_from(row)) else {
            return;
        };
        let Some(under) = self.buf.get(c, r).map(|cell| cell.bg) else {
            return;
        };
        let (top, bottom) = (under.under(top), under.under(bottom));
        if let Some(cell) = self.buf.get_mut(c, r) {
            cell.bg = bottom;
        }
        // Both halves alike need no glyph, and the space is what takes with it whatever the picture is covering.
        let glyph = if top == bottom {
            Grapheme::SPACE
        } else {
            Grapheme::from(UPPER_HALF)
        };
        self.buf.put(c, r, glyph, top, Attrs::NONE);
    }
}

/// The pixels of an image that has any, addressed in its own coordinates.
struct Source<'a> {
    pixels: &'a [u8],
    width: u32,
    height: u32,
}

impl<'a> Source<'a> {
    /// `None` for a picture this backend cannot read: a texture the application owns and keeps on a GPU, or a buffer too short for the dimensions it claims.
    fn of(data: &'a ImageData) -> Option<Self> {
        let needed = (data.width as usize)
            .checked_mul(data.height as usize)?
            .checked_mul(4)?;
        let pixels = data.pixels();
        (needed > 0 && pixels.len() >= needed).then_some(Self {
            pixels,
            width: data.width,
            height: data.height,
        })
    }

    /// The colour of the part of the picture that falls in `x` × `y`, which are half-cell edges in the surface's own coordinates.
    fn sample(&self, placed: &Rect, raster: Raster, x: (f32, f32), y: (f32, f32)) -> Color {
        let u = |v: f32| (v - placed.x) / placed.width * self.width as f32;
        let v = |value: f32| (value - placed.y) / placed.height * self.height as f32;
        let (x0, x1) = (u(x.0), u(x.1));
        let (y0, y1) = (v(y.0), v(y.1));
        let premultiplied = match raster {
            // Nearest, so artwork drawn on a pixel grid keeps its edges rather than being averaged into mush at the one size a terminal can show it.
            Raster::Pixel => self.nearest((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            Raster::Smooth => self.average(x0, x1, y0, y1),
        };
        straight(premultiplied)
    }

    fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        let channel = |i: usize| self.pixels[at + i] as f32 / 255.0;
        [channel(0), channel(1), channel(2), channel(3)]
    }

    fn nearest(&self, x: f32, y: f32) -> [f32; 4] {
        let clamp = |v: f32, limit: u32| (v.floor().max(0.0) as u32).min(limit - 1);
        self.pixel(clamp(x, self.width), clamp(y, self.height))
    }

    /// The mean of the pixels a half-cell covers, in premultiplied channels — which is the space an average of colours with different alphas is meaningful in.
    ///
    /// A terminal shows a picture at a fraction of the size it was drawn at, so almost every sample here is a downscale over many pixels and the mean is what keeps detail that nearest-neighbour would drop entirely. Where the footprint is smaller than a pixel the span collapses to the one it lands on, which is the nearest sample again.
    fn average(&self, x0: f32, x1: f32, y0: f32, y1: f32) -> [f32; 4] {
        let columns = span(x0, x1, self.width);
        let rows = span(y0, y1, self.height);
        let (dx, dy) = (stride(&columns), stride(&rows));
        let mut total = [0.0f32; 4];
        let mut count = 0.0;
        for y in rows.step_by(dy) {
            for x in columns.clone().step_by(dx) {
                let pixel = self.pixel(x, y);
                for (sum, channel) in total.iter_mut().zip(pixel) {
                    *sum += channel;
                }
                count += 1.0;
            }
        }
        total.map(|sum| sum / count)
    }
}

/// The pixels a half-cell reaches over, always at least one: a footprint narrower than a pixel is the pixel it lands in, and one that runs off the picture is the edge it ran off.
fn span(from: f32, to: f32, limit: u32) -> std::ops::Range<u32> {
    let first = (from.floor().max(0.0) as u32).min(limit - 1);
    let last = ((to.ceil().max(1.0) as u32).max(first + 1)).min(limit);
    first..last
}

/// How many pixels to skip between samples, so what a frame reads is bounded by the screen rather than by the file.
///
/// The downscale here is enormous — a 4000-pixel photograph across eighty columns is fifty source pixels per half-cell in each direction — and averaging every one of them is work whose result a cell has no way to show. A bounded grid inside the footprint lands on the same colour to within a rounding.
fn stride(over: &std::ops::Range<u32>) -> usize {
    (over.end - over.start)
        .div_ceil(SAMPLES_PER_HALF_CELL)
        .max(1) as usize
}

/// Premultiplied channels as the straight-alpha colour the rest of the painter blends in.
fn straight([r, g, b, a]: [f32; 4]) -> Color {
    if a <= 0.0 {
        return Color::TRANSPARENT;
    }
    Color::rgba(r / a, g / a, b / a, a)
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
