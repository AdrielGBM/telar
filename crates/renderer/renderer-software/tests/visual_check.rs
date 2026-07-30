//! Ad-hoc visual harness: renders a scene headless and writes a PNG to `TELAR_VISUAL_OUT`, so a
//! render change can be eyeballed (open the PNG) instead of only hashed. Run with:
//!   TELAR_VISUAL_OUT=/path/out.png cargo test -p renderer-software --test visual_check -- --nocapture

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, RenderBackend, TextStyle};
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

#[test]
fn visual_check_png() {
    let Ok(out) = std::env::var("TELAR_VISUAL_OUT") else {
        eprintln!("set TELAR_VISUAL_OUT to write a PNG; skipping");
        return;
    };

    let (w, h) = (520u32, 420u32);
    let mut r = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        w,
        h,
        SoftwareRendererConfig::default(),
    );

    let ink = Color::from_rgb_u8(230, 232, 240);
    let muted = Color::from_rgb_u8(150, 155, 168);
    let para = "This is a long paragraph that wraps across several lines so that line clamping and ellipsis truncation have something to actually cut off at the boundary.";

    // Each block: a label, then the paragraph in a 300px-wide box with a given clamp.
    let block = |cmds: &mut Vec<DrawCommand>, label: &str, y: f32, style: TextStyle| {
        cmds.push(DrawCommand::Text {
            text: Arc::from(label),
            rect: Rect::new(20.0, y, 480.0, 18.0),
            style: Arc::new(TextStyle::new(12.0, muted)),
        });
        cmds.push(DrawCommand::Text {
            text: Arc::from(para),
            rect: Rect::new(20.0, y + 20.0, 300.0, 200.0),
            style: Arc::new(style),
        });
    };

    let mut cmds = Vec::new();
    block(
        &mut cmds,
        "unclamped (wraps freely)",
        16.0,
        TextStyle::new(14.0, ink),
    );
    block(
        &mut cmds,
        "lines:2 (clamp, no ellipsis)",
        120.0,
        TextStyle::new(14.0, ink).with_max_lines(2),
    );
    block(
        &mut cmds,
        "lines:2 ellipsis",
        210.0,
        TextStyle::new(14.0, ink)
            .with_max_lines(2)
            .with_ellipsis(true),
    );
    block(
        &mut cmds,
        "lines:1 ellipsis (single-line truncation)",
        300.0,
        TextStyle::new(14.0, ink)
            .with_max_lines(1)
            .with_ellipsis(true),
    );

    r.begin_frame(w, h, 1.0, 0).unwrap();
    r.render_frame(&cmds, Some(Color::from_rgb_u8(20, 22, 28)))
        .unwrap();
    let rgba = r.read_rgba().expect("pixmap exists after a frame");

    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba length matches w*h*4");
    img.save(&out).expect("write PNG");
    eprintln!("wrote {out}");
}
