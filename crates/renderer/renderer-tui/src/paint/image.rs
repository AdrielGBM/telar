//! A picture in the only resolution a terminal has.

use geometry_core::Rect;
use renderer_core::{Color, ImageData, Raster};

use super::Painter;
use crate::cell::{Attrs, Grapheme};
use crate::color::Rgb;

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
mod tests {
    use std::sync::Arc;

    use renderer_core::{DrawCommand, TextStyle};

    use super::*;
    use crate::buffer::CellBuffer;
    use crate::metrics::CellSize;

    const CELL: CellSize = CellSize {
        width: 8.0,
        height: 16.0,
    };

    /// Built as already premultiplied, which for the opaque fixtures below they are: `ImageData::new` would premultiply them through its `>>8` approximation and turn every 255 into a 254 that says nothing about what this module did with it.
    fn image(pixels: &[[u8; 4]], width: u32, height: u32) -> Arc<ImageData> {
        let bytes: Vec<u8> = pixels.iter().flatten().copied().collect();
        Arc::new(ImageData::from_premultiplied(bytes, width, height))
    }

    fn draw(buf: &mut CellBuffer, commands: &[DrawCommand]) {
        Painter::new(buf, CELL, crate::ColorDepth::TrueColor).paint(commands);
    }

    fn picture(data: Arc<ImageData>, rect: Rect, raster: Raster) -> DrawCommand {
        DrawCommand::Image { data, rect, raster }
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    #[test]
    fn a_solid_picture_colours_every_cell_it_covers() {
        let mut buf = CellBuffer::new(4, 2, Rgb::BLACK);
        draw(
            &mut buf,
            &[picture(
                image(&[RED], 1, 1),
                Rect::new(0.0, 0.0, 16.0, 16.0),
                Raster::Smooth,
            )],
        );
        assert_eq!(buf.get(0, 0).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf.get(1, 0).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf.get(2, 0).unwrap().bg, Rgb::BLACK, "past its right edge");
        assert_eq!(
            buf.get(0, 1).unwrap().bg,
            Rgb::BLACK,
            "past its bottom edge"
        );
    }

    /// The whole point of the half block: one cell of a picture can be two colours, so a shape twice as tall as the grid still reads.
    #[test]
    fn a_cell_shows_both_of_its_halves() {
        let mut buf = CellBuffer::new(1, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[picture(
                image(&[RED, BLUE], 1, 2),
                Rect::new(0.0, 0.0, 8.0, 16.0),
                Raster::Smooth,
            )],
        );
        let cell = buf.get(0, 0).unwrap();
        assert_eq!(cell.glyph.as_str(), "▀");
        assert_eq!(cell.fg, Rgb { r: 255, g: 0, b: 0 }, "the top half");
        assert_eq!(cell.bg, Rgb { r: 0, g: 0, b: 255 }, "the bottom half");
    }

    /// A flat run of picture is a run of background, and the glyph goes with it — otherwise a picture laid over a paragraph reads as text on top of it.
    #[test]
    fn a_flat_cell_takes_the_glyph_under_it() {
        let mut buf = CellBuffer::new(4, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[
                DrawCommand::Text {
                    text: "abcd".into(),
                    spans: None,
                    rect: Rect::new(0.0, 0.0, 32.0, 16.0),
                    style: Arc::new(TextStyle::new(14.0, Color::WHITE)),
                },
                picture(
                    image(&[RED], 1, 1),
                    Rect::new(0.0, 0.0, 32.0, 16.0),
                    Raster::Smooth,
                ),
            ],
        );
        assert_eq!(buf.get(0, 0).unwrap().glyph.as_str(), " ");
        assert_eq!(buf.get(0, 0).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
    }

    /// Where a picture covers nothing it asks for nothing, so its transparent margin leaves the button it sits on alone.
    #[test]
    fn a_transparent_picture_leaves_what_is_under_it() {
        let mut buf = CellBuffer::new(2, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[
                DrawCommand::Text {
                    text: "ab".into(),
                    spans: None,
                    rect: Rect::new(0.0, 0.0, 16.0, 16.0),
                    style: Arc::new(TextStyle::new(14.0, Color::WHITE)),
                },
                picture(
                    image(&[CLEAR], 1, 1),
                    Rect::new(0.0, 0.0, 16.0, 16.0),
                    Raster::Smooth,
                ),
            ],
        );
        assert_eq!(buf.get(0, 0).unwrap().glyph.as_str(), "a");
        assert_eq!(buf.get(1, 0).unwrap().glyph.as_str(), "b");
    }

    /// Nearest keeps a hard edge hard. Averaged, a two-colour checker at this size comes out as one flat mixture in every cell.
    #[test]
    fn pixel_art_keeps_its_edges() {
        let mut buf = CellBuffer::new(2, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[picture(
                image(&[RED, BLUE, RED, BLUE], 2, 2),
                Rect::new(0.0, 0.0, 16.0, 16.0),
                Raster::Pixel,
            )],
        );
        assert_eq!(buf.get(0, 0).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf.get(1, 0).unwrap().bg, Rgb { r: 0, g: 0, b: 255 });
    }

    /// A picture shown far smaller than it was drawn has to average, or every cell but one in a photograph is whichever pixel happened to land under its centre.
    #[test]
    fn a_downscaled_picture_averages_what_it_drops() {
        let mut buf = CellBuffer::new(1, 1, Rgb::BLACK);
        let checker = [RED, BLUE, BLUE, RED];
        draw(
            &mut buf,
            &[picture(
                image(&checker, 2, 2),
                Rect::new(0.0, 0.0, 8.0, 8.0),
                Raster::Smooth,
            )],
        );
        let bg = buf.get(0, 0).unwrap().bg;
        assert!(bg.r > 0 && bg.b > 0, "neither colour survived: {bg:?}");
    }

    #[test]
    fn a_clip_cuts_the_picture() {
        let mut buf = CellBuffer::new(4, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[
                DrawCommand::PushClip {
                    rect: Rect::new(0.0, 0.0, 16.0, 16.0),
                    radius: renderer_core::BorderRadius::zero(),
                },
                picture(
                    image(&[RED], 1, 1),
                    Rect::new(0.0, 0.0, 32.0, 16.0),
                    Raster::Smooth,
                ),
                DrawCommand::PopClip,
            ],
        );
        assert_eq!(buf.get(1, 0).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(buf.get(2, 0).unwrap().bg, Rgb::BLACK, "outside the clip");
    }

    /// A layer's opacity is what the picture is drawn through, exactly as it is for a fill.
    #[test]
    fn a_layer_fades_the_picture() {
        let mut buf = CellBuffer::new(1, 1, Rgb::BLACK);
        draw(
            &mut buf,
            &[
                DrawCommand::PushLayer {
                    opacity: 0.5,
                    backdrop_blur: 0.0,
                },
                picture(
                    image(&[[255, 255, 255, 255]], 1, 1),
                    Rect::new(0.0, 0.0, 8.0, 16.0),
                    Raster::Smooth,
                ),
                DrawCommand::PopLayer,
            ],
        );
        let bg = buf.get(0, 0).unwrap().bg;
        assert!(bg.r.abs_diff(128) <= 1, "got {bg:?}");
    }

    /// A texture the application keeps on the GPU has no pixels here, and a backend that cannot read one draws nothing rather than a guess.
    #[test]
    fn a_texture_this_backend_cannot_read_draws_nothing() {
        #[derive(Debug)]
        struct Handle;
        impl renderer_core::ExternalTexture for Handle {
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        let mut buf = CellBuffer::new(2, 1, Rgb::BLACK);
        let data = Arc::new(ImageData::external(Arc::new(Handle), 1, 8, 16));
        draw(
            &mut buf,
            &[picture(
                data,
                Rect::new(0.0, 0.0, 16.0, 16.0),
                Raster::Smooth,
            )],
        );
        assert_eq!(buf.get(0, 0).unwrap().bg, Rgb::BLACK);
    }
}
