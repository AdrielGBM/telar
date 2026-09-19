use super::*;
use geometry_core::Rect;
use renderer_core::{RectStyle, ShapeStyle};
use std::sync::Arc;

fn red_square(size: f32) -> Vec<DrawCommand> {
    vec![DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, size, size),
        style: Arc::new(RectStyle::default().with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0))),
    }]
}

#[test]
fn it_draws_the_commands_it_is_given() {
    let pixels = rasterize(&red_square(8.0), 8, 8, 1.0, None).expect("pixels");
    assert_eq!(pixels.len(), 8 * 8 * 4);
    let first = &pixels[0..4];
    assert_eq!(
        (first[0], first[3]),
        (255, 255),
        "an opaque red square must reach the buffer"
    );
}

// `None` is documented as fully transparent, which is what a drag icon needs: the compositor blends it, so a black background would be a black box following the cursor.
fn transparent_corner(pixels: &[u8]) -> u8 {
    pixels[pixels.len() - 1]
}

#[test]
fn an_unset_clear_colour_leaves_the_background_transparent() {
    let pixels = rasterize(&red_square(4.0), 16, 16, 1.0, None).expect("pixels");
    assert_eq!(
        transparent_corner(&pixels),
        0,
        "the area the commands did not cover stays clear"
    );
}

#[test]
fn an_empty_size_is_none_rather_than_a_failure() {
    assert!(
        rasterize(&red_square(4.0), 0, 8, 1.0, None).is_none(),
        "a zero width has no pixels to rasterise"
    );
    assert!(
        rasterize(&red_square(4.0), 8, 0, 1.0, None).is_none(),
        "and neither has a zero height"
    );
}

// The renderer is cached per thread and resized as needed, so a second call at a different size must produce a buffer for that size.
#[test]
fn a_cached_renderer_still_resizes_between_calls() {
    let small = rasterize(&red_square(4.0), 8, 8, 1.0, None).expect("small");
    let large = rasterize(&red_square(4.0), 32, 24, 1.0, None).expect("large");
    assert_eq!(small.len(), 8 * 8 * 4);
    assert_eq!(large.len(), 32 * 24 * 4);
}

const RED: [u8; 4] = [220, 40, 40, 255];
const GREEN: [u8; 4] = [40, 200, 60, 255];
const BLUE: [u8; 4] = [40, 60, 220, 255];
const YELLOW: [u8; 4] = [230, 200, 40, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn picture(pixels: &[[u8; 4]], width: u32, height: u32) -> Arc<crate::ImageData> {
    Arc::new(crate::ImageData::from_premultiplied(
        pixels.concat(),
        width,
        height,
    ))
}

fn image(data: Arc<crate::ImageData>, rect: Rect, fill: crate::ImageFill) -> DrawCommand {
    DrawCommand::Image {
        data,
        rect,
        raster: crate::Raster::Pixel,
        fill,
    }
}

#[test]
fn a_tile_repeats_the_picture_at_its_scaled_size_from_the_rect_origin() {
    let quadrants = picture(&[RED, GREEN, BLUE, YELLOW], 2, 2);
    let commands = [image(
        quadrants,
        Rect::new(3.0, 2.0, 32.0, 24.0),
        crate::ImageFill::Tile { scale: 4.0 },
    )];
    let pixels = rasterize(&commands, 40, 30, 1.0, Some(Color::BLACK)).expect("pixels");
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
                pixel(&pixels, 40, x, y),
                expected,
                "at ({x}, {y}): an 8 px period anchored at the rect's corner"
            );
        }
    }
    assert_eq!(pixel(&pixels, 40, 2, 2), BLACK, "nothing left of the rect");
    assert_eq!(pixel(&pixels, 40, 35, 25), BLACK, "nothing right of it");
}

#[test]
fn a_nine_slice_keeps_its_corners_and_stretches_its_edges() {
    // 4×4: a 1 px frame of red corners and green edges around a blue middle.
    let (r, g, b) = (RED, GREEN, BLUE);
    let framed = picture(&[r, g, g, r, g, b, b, g, g, b, b, g, r, g, g, r], 4, 4);
    let slice = crate::ImageSlice::new(crate::Insets::all(1.0)).with_scale(3.0);
    let commands = [image(
        framed,
        Rect::new(2.0, 2.0, 30.0, 20.0),
        crate::ImageFill::Slice(slice),
    )];
    let pixels = rasterize(&commands, 34, 24, 1.0, Some(Color::BLACK)).expect("pixels");
    let at = |x: u32, y: u32| pixel(&pixels, 34, 2 + x, 2 + y);
    for (x, y) in [(0, 0), (2, 2), (27, 0), (29, 2), (0, 17), (29, 19)] {
        assert_eq!(at(x, y), RED, "({x}, {y}) is inside a 3 px corner");
    }
    for (x, y) in [(3, 0), (26, 2), (0, 3), (2, 16), (29, 10), (15, 19)] {
        assert_eq!(
            at(x, y),
            GREEN,
            "({x}, {y}) is on an edge, stretched along it"
        );
    }
    for (x, y) in [(3, 3), (26, 16), (15, 10)] {
        assert_eq!(at(x, y), BLUE, "({x}, {y}) is in the middle");
    }
}

#[test]
fn a_blended_layer_lands_the_formula_on_known_pixels() {
    let gray = Color::from_rgb_u8(128, 128, 128);
    let red = Color::from_rgb_u8(255, 0, 0);
    let cases = [
        (crate::BlendMode::Multiply, [128, 0, 0]),
        (crate::BlendMode::Screen, [255, 128, 128]),
        (crate::BlendMode::Difference, [127, 128, 128]),
        (crate::BlendMode::Plus, [255, 128, 128]),
    ];
    for (blend, expected) in cases {
        let commands = [
            DrawCommand::Rect {
                rect: Rect::new(0.0, 0.0, 16.0, 16.0),
                style: Arc::new(RectStyle::default().with_fill(gray)),
            },
            DrawCommand::PushLayer {
                opacity: 1.0,
                backdrop_blur: 0.0,
                blend,
            },
            DrawCommand::Rect {
                rect: Rect::new(4.0, 4.0, 8.0, 8.0),
                style: Arc::new(RectStyle::default().with_fill(red)),
            },
            DrawCommand::PopLayer,
        ];
        let pixels = rasterize(&commands, 16, 16, 1.0, Some(Color::BLACK)).expect("pixels");
        let got = pixel(&pixels, 16, 8, 8);
        for channel in 0..3 {
            assert!(
                got[channel].abs_diff(expected[channel]) <= 1,
                "{blend:?}: expected {expected:?}, got {got:?}"
            );
        }
        assert_eq!(
            pixel(&pixels, 16, 1, 1)[..3],
            [128, 128, 128],
            "{blend:?}: the backdrop outside the layer is untouched"
        );
    }
}
