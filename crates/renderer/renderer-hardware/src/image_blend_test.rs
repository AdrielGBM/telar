//! Tiled and nine-sliced pictures and blended layers on the GPU, asserted on geometry and known pixels rather than goldens, and every blend mode checked against the software backend drawing the same frame. Skips when no GPU adapter is available, unless `TELAR_REQUIRE_GPU` says one was expected.

#[path = "test_common.rs"]
mod common;

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{
    BlendMode, Color, DrawCommand, ImageData, ImageFill, ImageSlice, Insets, Raster, RectStyle,
    RenderBackend, ShapeStyle,
};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use renderer_text::TextShaperConfig;
use telar_renderer_hardware::HardwareRenderer;

const RED: [u8; 4] = [220, 40, 40, 255];
const GREEN: [u8; 4] = [40, 200, 60, 255];
const BLUE: [u8; 4] = [40, 60, 220, 255];
const YELLOW: [u8; 4] = [230, 200, 40, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const MAGENTA: [u8; 4] = [200, 40, 200, 255];
const CYAN: [u8; 4] = [40, 200, 200, 255];
const GRAY: [u8; 4] = [120, 120, 120, 255];

struct Frame {
    pixels: Vec<u8>,
    width: u32,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }
}

fn gpu(width: u32, height: u32, commands: &[DrawCommand], what: &str) -> Option<Frame> {
    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
        width,
        height,
        None,
        false,
        TextShaperConfig::default(),
    )) {
        Ok(renderer) => renderer,
        Err(e) => {
            common::skip_without_gpu(what, e);
            return None;
        }
    };
    renderer
        .begin_frame(width, height, 1.0, 1)
        .expect("begin_frame");
    renderer
        .render_frame(commands, Some(Color::BLACK))
        .expect("render_frame");
    Some(Frame {
        pixels: renderer.read_rgba().expect("read_rgba"),
        width,
    })
}

fn cpu(width: u32, height: u32, commands: &[DrawCommand]) -> Frame {
    let mut renderer: SoftwareRenderer<HeadlessWindow, HeadlessWindow> =
        SoftwareRenderer::new_headless(width, height, SoftwareRendererConfig::default());
    renderer
        .begin_frame(width, height, 1.0, 0)
        .expect("begin_frame");
    renderer
        .render_frame(commands, Some(Color::BLACK))
        .expect("render_frame");
    Frame {
        pixels: renderer.read_rgba().expect("read_rgba").to_vec(),
        width,
    }
}

fn picture(pixels: &[[u8; 4]], width: u32, height: u32) -> Arc<ImageData> {
    Arc::new(ImageData::from_premultiplied(
        pixels.concat(),
        width,
        height,
    ))
}

fn image(data: Arc<ImageData>, rect: Rect, fill: ImageFill) -> DrawCommand {
    DrawCommand::Image {
        data,
        rect,
        raster: Raster::Pixel,
        fill,
    }
}

fn filled(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: Arc::new(RectStyle::default().with_fill(color)),
    }
}

fn layer(opacity: f32, blend: BlendMode) -> DrawCommand {
    DrawCommand::PushLayer {
        opacity,
        backdrop_blur: 0.0,
        blend,
    }
}

#[test]
fn a_tile_repeats_the_picture_at_its_scaled_size_from_the_rect_origin() {
    let quadrants = picture(&[RED, GREEN, BLUE, YELLOW], 2, 2);
    let commands = [image(
        quadrants,
        Rect::new(3.0, 2.0, 32.0, 24.0),
        ImageFill::Tile { scale: 4.0 },
    )];
    let Some(frame) = gpu(40, 30, &commands, "tile period") else {
        return;
    };
    for y in 2..26 {
        for x in 3..35 {
            let (u, v) = ((x - 3) % 8, (y - 2) % 8);
            let expected = match (u < 4, v < 4) {
                (true, true) => RED,
                (false, true) => GREEN,
                (true, false) => BLUE,
                (false, false) => YELLOW,
            };
            assert_eq!(
                frame.at(x, y),
                expected,
                "at ({x}, {y}): an 8 px period anchored at (3, 2)"
            );
        }
    }
    assert_eq!(frame.at(2, 2), BLACK, "nothing past the rect's left edge");
    assert_eq!(frame.at(35, 25), BLACK, "nothing past its right edge");
}

/// A 6×6 frame: 2 px corners in four colours, edges in four more, a black middle.
fn frame_picture() -> Arc<ImageData> {
    let mut pixels = Vec::new();
    for y in 0..6 {
        for x in 0..6 {
            let column = if x < 2 {
                0
            } else if x < 4 {
                1
            } else {
                2
            };
            let row = if y < 2 {
                0
            } else if y < 4 {
                1
            } else {
                2
            };
            pixels.push(match (column, row) {
                (0, 0) => RED,
                (2, 0) => GREEN,
                (0, 2) => BLUE,
                (2, 2) => YELLOW,
                (1, 0) => WHITE,
                (0, 1) => MAGENTA,
                (2, 1) => CYAN,
                (1, 2) => GRAY,
                _ => BLACK,
            });
        }
    }
    picture(&pixels, 6, 6)
}

fn assert_nine_slice(frame: &Frame, origin: (u32, u32), size: (u32, u32), what: &str) {
    let (ox, oy) = origin;
    let (w, h) = size;
    let region = |x: u32, y: u32| {
        let column = if x < 2 {
            0
        } else if x < w - 2 {
            1
        } else {
            2
        };
        let row = if y < 2 {
            0
        } else if y < h - 2 {
            1
        } else {
            2
        };
        (column, row)
    };
    for y in 0..h {
        for x in 0..w {
            let expected = match region(x, y) {
                (0, 0) => RED,
                (2, 0) => GREEN,
                (0, 2) => BLUE,
                (2, 2) => YELLOW,
                (1, 0) => WHITE,
                (0, 1) => MAGENTA,
                (2, 1) => CYAN,
                (1, 2) => GRAY,
                _ => BLACK,
            };
            assert_eq!(
                frame.at(ox + x, oy + y),
                expected,
                "{what}: at ({x}, {y}) of the rect, corners 2 px and unscaled, edges stretched"
            );
        }
    }
}

#[test]
fn a_nine_slice_keeps_its_corners_and_stretches_its_edges() {
    let commands = [image(
        frame_picture(),
        Rect::new(4.0, 3.0, 40.0, 24.0),
        ImageFill::Slice(ImageSlice::new(Insets::all(2.0))),
    )];
    let Some(frame) = gpu(48, 32, &commands, "nine-slice") else {
        return;
    };
    assert_nine_slice(&frame, (4, 3), (40, 24), "gpu");
    assert_nine_slice(&cpu(48, 32, &commands), (4, 3), (40, 24), "cpu");
}

#[test]
fn a_blended_layer_lands_the_formula_on_known_pixels() {
    let gray = Color::from_rgb_u8(128, 128, 128);
    let red = Color::from_rgb_u8(255, 0, 0);
    let cases = [
        (BlendMode::Multiply, [128, 0, 0]),
        (BlendMode::Screen, [255, 128, 128]),
        (BlendMode::Difference, [127, 128, 128]),
        (BlendMode::Plus, [255, 128, 128]),
        (BlendMode::Darken, [128, 0, 0]),
        (BlendMode::Lighten, [255, 128, 128]),
    ];
    for (blend, expected) in cases {
        let commands = [
            filled(Rect::new(0.0, 0.0, 16.0, 16.0), gray),
            layer(1.0, blend),
            filled(Rect::new(4.0, 4.0, 8.0, 8.0), red),
            DrawCommand::PopLayer,
        ];
        let Some(frame) = gpu(16, 16, &commands, "blend on known pixels") else {
            return;
        };
        let got = frame.at(8, 8);
        for channel in 0..3 {
            assert!(
                got[channel].abs_diff(expected[channel]) <= 1,
                "{blend:?}: expected {expected:?}, got {got:?}"
            );
        }
        assert_eq!(
            frame.at(1, 1)[..3],
            [128, 128, 128],
            "{blend:?}: the backdrop outside the layer is untouched"
        );
    }
}

/// Premultiplied stripes, 4 px each, from edge to edge of a `length`-long picture that is `across` wide; vertical when `vertical`.
fn stripes(colors: &[[u8; 4]], across: u32, length: u32, vertical: bool) -> Arc<ImageData> {
    let (width, height) = if vertical {
        (length, across)
    } else {
        (across, length)
    };
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let along = if vertical { x } else { y };
            pixels.push(colors[(along / 4) as usize % colors.len()]);
        }
    }
    picture(&pixels, width, height)
}

/// Crossing stripes of colour and alpha, backdrop against layer, so every mode meets dark and light, opaque and translucent, and the edge cases its formula branches on (a zero channel, a channel equal to its alpha) on both sides; `nested` composites into a translucent parent layer, where the backdrop's alpha is below one. Both are pictures rather than filled rects so the two backends start from the same bytes, since a fill rounds to 8 bits slightly differently in each.
fn blend_scene(blend: BlendMode, nested: bool) -> Vec<DrawCommand> {
    let backdrop = stripes(
        &[
            [230, 50, 25, 255],
            [15, 110, 45, 153],
            [30, 30, 77, 77],
            [242, 230, 51, 255],
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [0, 90, 120, 200],
            [128, 128, 128, 255],
        ],
        32,
        32,
        true,
    );
    let source = stripes(
        &[
            [51, 77, 204, 255],
            [160, 105, 18, 178],
            [51, 51, 51, 102],
            [13, 230, 230, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 255],
            [100, 0, 100, 100],
            [60, 60, 0, 128],
        ],
        28,
        32,
        false,
    );
    let mut commands = Vec::new();
    if nested {
        commands.push(layer(1.0, BlendMode::Normal));
    }
    commands.push(image(
        backdrop,
        Rect::new(0.0, 0.0, 32.0, 32.0),
        ImageFill::Stretch,
    ));
    commands.push(layer(0.8, blend));
    commands.push(image(
        source,
        Rect::new(2.0, 0.0, 28.0, 32.0),
        ImageFill::Stretch,
    ));
    commands.push(DrawCommand::PopLayer);
    if nested {
        commands.push(DrawCommand::PopLayer);
    }
    commands
}

#[test]
fn every_blend_mode_matches_the_software_backend() {
    let mut worst = Vec::new();
    for nested in [false, true] {
        for blend in BlendMode::ALL {
            let commands = blend_scene(blend, nested);
            let Some(gpu_frame) = gpu(32, 32, &commands, "blend parity") else {
                return;
            };
            let cpu_frame = cpu(32, 32, &commands);
            let difference = gpu_frame
                .pixels
                .iter()
                .zip(&cpu_frame.pixels)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            worst.push((blend, nested, difference));
        }
    }
    let off: Vec<_> = worst.iter().filter(|(_, _, d)| *d > 3).collect();
    assert!(
        off.is_empty(),
        "modes the two backends draw differently (mode, nested, worst channel difference): {off:?}"
    );
}
