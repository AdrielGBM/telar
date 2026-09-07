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
