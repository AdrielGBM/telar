//! What a per-side border has to be true of, asserted on the pixels the rasterizer actually produced.
//!
//! The unit tests around `border_inner_shape` prove the geometry; these prove it reached the buffer. The two
//! failures worth catching here are the ones a geometry test cannot see: a side the author left at zero
//! painted anyway, and the ring collapsing so that nothing is painted at all.

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{
    BorderRadius, BorderWidths, Color, DrawCommand, RectStyle, RenderBackend, ShapeStyle, Stroke,
};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

const W: u32 = 40;
const H: u32 = 40;
const BOX_RECT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 40.0,
    height: 40.0,
};

/// Renders one rect over black and hands back the buffer.
fn render(style: RectStyle) -> Vec<u8> {
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        W,
        H,
        SoftwareRendererConfig::default(),
    );
    let cmds = vec![DrawCommand::Rect {
        rect: BOX_RECT,
        style: Arc::new(style),
    }];
    renderer.begin_frame(W, H, 1.0, 0).unwrap();
    renderer.render_frame(&cmds, Some(Color::BLACK)).unwrap();
    renderer.read_rgba().expect("a frame was rendered").to_vec()
}

/// The red channel at a pixel — the border here is pure red, and the background pure black, so this reads as
/// "how much border landed on this pixel".
fn red_at(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * W + x) * 4) as usize]
}

fn bordered(widths: BorderWidths) -> RectStyle {
    RectStyle::default()
        .with_stroke(Stroke::new(Color::RED, 1.0))
        .with_border_widths(widths)
}

/// The whole feature in one assertion: a rule under a header paints its own edge and leaves the other three
/// alone. A stroke that ignored the sides would light up all four.
#[test]
fn a_bottom_border_paints_the_bottom_edge_and_nothing_else() {
    let pixels = render(bordered(BorderWidths::per_side(0.0, 0.0, 1.0, 0.0)));

    assert!(
        red_at(&pixels, 20, 39) > 200,
        "the bottom row carries the rule"
    );
    for (x, y, edge) in [(20, 0, "top"), (39, 20, "right"), (0, 20, "left")] {
        assert_eq!(
            red_at(&pixels, x, y),
            0,
            "the {edge} edge was never asked for"
        );
    }
}

/// The same box under the uniform form still frames all four sides, so the per-side path did not become the
/// only path.
#[test]
fn a_plain_stroke_still_frames_the_whole_box() {
    let pixels = render(bordered(BorderWidths::Uniform));

    for (x, y, edge) in [
        (20, 0, "top"),
        (39, 20, "right"),
        (20, 39, "bottom"),
        (0, 20, "left"),
    ] {
        assert!(
            red_at(&pixels, x, y) > 200,
            "the {edge} edge belongs to a uniform border"
        );
    }
    assert_eq!(red_at(&pixels, 20, 20), 0, "the middle is still interior");
}

/// Two sides at different thicknesses, which is what the four-value shorthand is for and what a single
/// "which sides" flag could never express.
#[test]
fn sides_keep_their_own_thicknesses() {
    let pixels = render(bordered(BorderWidths::per_side(4.0, 0.0, 1.0, 0.0)));

    assert!(red_at(&pixels, 20, 3) > 200, "the top is four rows deep");
    assert_eq!(red_at(&pixels, 20, 4), 0, "and stops after the fourth");
    assert!(red_at(&pixels, 20, 39) > 200, "the bottom is one row");
    assert_eq!(red_at(&pixels, 20, 38), 0, "and no more than one");
}

/// A rounded box with one side has to keep the corner arc that side runs into, or the rule ends in a notch.
#[test]
fn a_rounded_box_keeps_the_arc_its_one_side_runs_into() {
    let pixels = render(
        bordered(BorderWidths::per_side(0.0, 0.0, 2.0, 2.0)).with_radius(BorderRadius::all(8.0)),
    );

    assert!(
        red_at(&pixels, 3, 36) > 100,
        "the bottom-left corner is where the two drawn sides meet, and it is drawn"
    );
    assert_eq!(
        red_at(&pixels, 36, 3),
        0,
        "the opposite corner has neither side and stays clear"
    );
}

/// The degenerate end of the range: a border thicker than the box it frames has no interior to punch out, so
/// it fills the box rather than vanishing.
#[test]
fn a_border_thicker_than_its_box_fills_it() {
    let pixels = render(bordered(BorderWidths::per_side(30.0, 0.0, 30.0, 0.0)));

    for y in [0, 20, 39] {
        assert!(
            red_at(&pixels, 20, y) > 200,
            "row {y} is inside a border that swallowed the box"
        );
    }
}

/// The fill stops where the border starts on the sides that have one, and reaches the edge on the sides that
/// do not — the same interior both backends derive from `border_inner_shape`.
#[test]
fn the_fill_meets_the_border_only_where_there_is_one() {
    let mut style = bordered(BorderWidths::per_side(0.0, 0.0, 6.0, 0.0));
    style.fill = Some(Color::from_rgb_u8(0, 0, 255).into());
    let pixels = render(style);

    let blue_at = |x: u32, y: u32| pixels[((y * W + x) * 4 + 2) as usize];
    assert!(
        blue_at(20, 0) > 200,
        "no top border, so the fill reaches the top edge"
    );
    assert!(
        red_at(&pixels, 20, 39) > 200,
        "the bottom six rows are border"
    );
    assert!(
        blue_at(20, 39) < 50,
        "and the fill does not show through them"
    );
}
