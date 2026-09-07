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
