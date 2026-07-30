//! Regression: on a TRANSPARENT surface (clear_color None), the incremental damage path must still clear
//! the pixels a moved element vacated. Otherwise a reflow — e.g. adding/removing a bar module shifts its
//! neighbours — leaves the old frame behind as a ghost ("the icon after the icon").

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, RectStyle, RenderBackend, ShapeStyle};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

fn alpha_at(rgba: &[u8], w: u32, x: u32, y: u32) -> u8 {
    rgba[((y * w + x) * 4 + 3) as usize]
}

#[test]
fn transparent_surface_clears_vacated_pixels_on_reflow() {
    let (w, h) = (64u32, 16u32);
    let mut r = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );
    let chip = |x: f32| DrawCommand::Rect {
        rect: Rect::new(x, 0.0, 16.0, 16.0),
        style: Arc::new(RectStyle::default().with_fill(Color::from_rgb_u8(200, 100, 80))),
    };

    // Frame 0: chip at the left. Transparent surface → clear_color None.
    r.begin_frame(w, h, 1.0, 0).unwrap();
    r.render_frame(&[chip(0.0)], None).unwrap();
    let f0 = r.read_rgba().unwrap().to_vec();
    assert!(
        alpha_at(&f0, w, 8, 8) > 200,
        "chip is opaque at x=8 in frame 0"
    );
    assert_eq!(
        alpha_at(&f0, w, 48, 8),
        0,
        "right side still transparent in frame 0"
    );

    // Frame 1: the chip reflows to the right (same renderer → incremental damage path).
    r.begin_frame(w, h, 1.0, 1).unwrap();
    r.render_frame(&[chip(40.0)], None).unwrap();
    let f1 = r.read_rgba().unwrap().to_vec();
    assert!(
        alpha_at(&f1, w, 48, 8) > 200,
        "chip is now opaque at its new x=48"
    );
    assert_eq!(
        alpha_at(&f1, w, 8, 8),
        0,
        "the pixels the chip vacated (x=8) must be cleared to transparent, not left as a ghost"
    );
}
