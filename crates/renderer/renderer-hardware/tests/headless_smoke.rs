//! Smoke test for the headless (offscreen) rendering path: build a windowless renderer, render a
//! simple frame, and confirm `read_rgba` returns a tightly-packed, non-zero RGBA buffer with the
//! drawn rect distinguishable from the cleared background. Skips gracefully when no GPU adapter is
//! available (e.g. CI without a GPU) instead of failing.

use std::sync::Arc;

use geometry_core::{Point, Rect};
use renderer_core::{
    Color, DrawCommand, PathData, PathStyle, RectStyle, RenderBackend, Shadow, ShapeStyle,
    TextStyle,
};
use renderer_hardware::{HardwareRenderer, HardwareRendererConfig};
use renderer_text::TextShaperConfig;

#[test]
fn headless_renders_non_empty_frame() {
    let w = 64u32;
    let h = 48u32;

    let mut renderer = match pollster::block_on(HardwareRenderer::new_headless(
        w,
        h,
        None,
        false,
        TextShaperConfig::default(),
        HardwareRendererConfig::default(),
    )) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping headless smoke test: no GPU adapter available: {e:?}");
            return;
        }
    };

    let cmds = vec![DrawCommand::Rect {
        rect: Rect::new(8.0, 8.0, 40.0, 30.0),
        style: Arc::new(RectStyle::filled(Color::rgb(0.2, 0.6, 0.9), 4.0)),
    }];

    renderer.begin_frame(w, h, 1.0, 1).expect("begin_frame");
    renderer
        .render_frame(&cmds, Some(Color::rgb(0.05, 0.05, 0.08)))
        .expect("render_frame");

    let pixels = renderer.read_rgba().expect("read_rgba");
    assert_eq!(
        pixels.len(),
        (w * h * 4) as usize,
        "read_rgba must be tightly packed width*height*4"
    );
    assert!(
        pixels.iter().any(|&b| b != 0),
        "rendered frame is entirely zero — nothing was drawn"
    );

    // The rect (8,8 .. 48,38) covers the center; the (0,0) corner is background clear color.
    let center = (((h / 2) * w + (w / 2)) * 4) as usize;
    assert_ne!(
        &pixels[center..center + 3],
        &pixels[0..3],
        "rect region should differ from cleared background"
    );
}

// FNV-1a 64-bit over the raw RGBA bytes; a stable, allocation-free content hash for the golden check.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// A fixed, representative "dense UI" scene exercising every major primitive path plus shadow resolve
// (text + path) and a translucent backdrop-blur overlay layer. Mirrors benches/render_frame.rs but
// adds an explicitly shadowed text and path so ShadowKind::Text and ShadowKind::Path both fire.
fn golden_scene() -> Vec<DrawCommand> {
    const W: f32 = 1280.0;
    const H: f32 = 800.0;
    let mut cmds = Vec::new();

    // Opaque background fill.
    cmds.push(DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, W, H),
        style: Arc::new(RectStyle::filled(Color::from_rgb_u8(24, 24, 32), 0.0)),
    });

    // Grid of shadowed rounded cards, each with a text label.
    let card_style = Arc::new(
        RectStyle::filled(Color::from_rgb_u8(52, 58, 74), 8.0).with_shadow(Shadow::new(
            0.0,
            4.0,
            12.0,
            Color::rgba(0.0, 0.0, 0.0, 0.5),
        )),
    );
    let text_style = Arc::new(TextStyle::new(14.0, Color::WHITE));
    let cols = 8;
    let rows = 6;
    let pad = 16.0;
    let cell_w = (W - pad) / cols as f32;
    let cell_h = ((H - pad) * 0.85) / rows as f32;
    for r in 0..rows {
        for col in 0..cols {
            let x = pad + col as f32 * cell_w;
            let y = pad + r as f32 * cell_h;
            cmds.push(DrawCommand::Rect {
                rect: Rect::new(x, y, cell_w - pad, cell_h - pad),
                style: card_style.clone(),
            });
            cmds.push(DrawCommand::Text {
                text: Arc::from("Widget label"),
                rect: Rect::new(x + 12.0, y + 12.0, cell_w - pad - 24.0, 20.0),
                style: text_style.clone(),
            });
        }
    }

    // A shadowed heading: exercises the ShadowKind::Text resolve path.
    let heading_style = Arc::new(TextStyle {
        shadow: Some(Shadow::new(1.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.7))),
        ..TextStyle::new(22.0, Color::WHITE)
    });
    cmds.push(DrawCommand::Text {
        text: Arc::from("Dashboard"),
        rect: Rect::new(24.0, H - 60.0, 400.0, 30.0),
        style: heading_style,
    });

    // A row of filled triangles to exercise path tessellation.
    let path_style = Arc::new(PathStyle::default().with_fill(Color::from_rgb_u8(120, 200, 255)));
    for i in 0..24 {
        let cx = 40.0 + i as f32 * 50.0;
        let path = PathData::polygon(&[
            Point::new(cx, H - 70.0),
            Point::new(cx + 22.0, H - 24.0),
            Point::new(cx - 22.0, H - 24.0),
        ]);
        cmds.push(DrawCommand::Path {
            data: Arc::new(path),
            style: path_style.clone(),
        });
    }

    // A shadowed path: exercises the ShadowKind::Path resolve path.
    let shadow_path_style = Arc::new(
        PathStyle::default()
            .with_fill(Color::from_rgb_u8(255, 180, 80))
            .with_shadow(Shadow::new(2.0, 3.0, 6.0, Color::rgba(0.0, 0.0, 0.0, 0.5))),
    );
    let star = PathData::polygon(&[
        Point::new(1180.0, 690.0),
        Point::new(1200.0, 740.0),
        Point::new(1150.0, 710.0),
        Point::new(1210.0, 710.0),
        Point::new(1160.0, 740.0),
    ]);
    cmds.push(DrawCommand::Path {
        data: Arc::new(star),
        style: shadow_path_style,
    });

    // Translucent overlay panel with a backdrop blur: exercises layer capture, blur, and composite.
    cmds.push(DrawCommand::PushLayer {
        opacity: 0.85,
        backdrop_blur: 12.0,
    });
    cmds.push(DrawCommand::Rect {
        rect: Rect::new(320.0, 240.0, 640.0, 320.0),
        style: Arc::new(RectStyle::filled(Color::rgba(0.1, 0.1, 0.15, 0.6), 16.0)),
    });
    for i in 0..6 {
        cmds.push(DrawCommand::Text {
            text: Arc::from("Overlay content line"),
            rect: Rect::new(360.0, 280.0 + i as f32 * 32.0, 560.0, 24.0),
            style: text_style.clone(),
        });
    }
    cmds.push(DrawCommand::PopLayer);

    cmds
}

/// Golden pixel-hash guardrail for the `render_frame` decomposition: renders a fixed scene headless
/// and asserts the readback bytes hash to a baked constant. This hash must remain identical before
/// and after the method is split into phases — any change means behavior diverged.
#[test]
fn render_frame_pixel_golden() {
    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 800;
    // Baked from the pre-decomposition renderer on this machine's GPU; must not change post-split.
    const EXPECTED: u64 = 0x3507_0457_a257_bc16;

    let mut renderer = match pollster::block_on(HardwareRenderer::new_headless(
        WIDTH,
        HEIGHT,
        None,
        false,
        TextShaperConfig::default(),
        HardwareRendererConfig::default(),
    )) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping golden pixel test: no GPU adapter available: {e:?}");
            return;
        }
    };

    let cmds = golden_scene();
    let clear = Some(Color::from_rgb_u8(18, 18, 24));

    renderer
        .begin_frame(WIDTH, HEIGHT, 1.0, 1)
        .expect("begin_frame");
    renderer.render_frame(&cmds, clear).expect("render_frame");

    let pixels = renderer.read_rgba().expect("read_rgba");
    assert_eq!(
        pixels.len(),
        (WIDTH * HEIGHT * 4) as usize,
        "readback must be tightly packed width*height*4"
    );

    let hash = fnv1a_64(&pixels);
    assert_eq!(
        hash, EXPECTED,
        "golden pixel hash changed: render_frame behavior differs from baseline (got {hash:#018x})"
    );
}
