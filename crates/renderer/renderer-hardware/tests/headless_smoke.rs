//! Smoke test for the headless (offscreen) rendering path: build a windowless renderer, render a
//! simple frame, and confirm `read_rgba` returns a tightly-packed, non-zero RGBA buffer with the
//! drawn rect distinguishable from the cleared background. Skips gracefully when no GPU adapter is
//! available (e.g. CI without a GPU) instead of failing.

use std::sync::Arc;

use geometry_core::{Point, Rect};
use platform_headless::HeadlessWindow;
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

    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
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

// A layer-free grid of solid cells over an opaque background; `highlight` recolors exactly one cell
// so two renders differing only in `highlight` produce a single-cell dirty rect — the F1 scenario.
fn grid_scene(highlight: Option<usize>) -> Vec<DrawCommand> {
    const W: f32 = 800.0;
    const H: f32 = 600.0;
    let cols = 10usize;
    let rows = 8usize;
    let pad = 8.0f32;
    let cw = (W - pad) / cols as f32;
    let ch = (H - pad) / rows as f32;
    let mut cmds = vec![DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, W, H),
        style: Arc::new(RectStyle::filled(Color::from_rgb_u8(20, 20, 28), 0.0)),
    }];
    for r in 0..rows {
        for c in 0..cols {
            let idx = r * cols + c;
            let x = pad + c as f32 * cw;
            let y = pad + r as f32 * ch;
            let color = if highlight == Some(idx) {
                Color::from_rgb_u8(240, 60, 60)
            } else {
                Color::from_rgb_u8(60, 70, 90)
            };
            cmds.push(DrawCommand::Rect {
                rect: Rect::new(x, y, cw - pad, ch - pad),
                style: Arc::new(RectStyle::filled(color, 4.0)),
            });
        }
    }
    cmds
}

/// F1 correctness: rendering frame B *after* frame A (so B goes through the damage-prime path —
/// prime the retained frame A, repaint only the changed cell) must produce the same pixels as
/// rendering B from scratch (a full repaint). A missing or ghosted cell would leave a whole cell
/// (~1.6% of the surface) diverging, far above the MSAA-edge tolerance. Skips without a GPU, and is
/// only exercised on the MSAA path (`msaa_samples > 1`); on single-sample GPUs F1 no-ops and the two
/// renders are trivially equal.
#[test]
fn damage_prime_matches_full_repaint() {
    const W: u32 = 800;
    const H: u32 = 600;
    let clear = Some(Color::from_rgb_u8(20, 20, 28));
    let scene_a = grid_scene(None);
    let scene_b = grid_scene(Some(3 * 10 + 4)); // an interior cell, well away from the edges

    let make = || {
        pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
            W,
            H,
            None,
            false,
            TextShaperConfig::default(),
            HardwareRendererConfig::default(),
        ))
    };

    let mut r1 = match make() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping F1 pixel test: no GPU adapter available: {e:?}");
            return;
        }
    };
    // Frame A establishes prev_commands + the retained texture, then B triggers the damage prime.
    r1.begin_frame(W, H, 1.0, 1).expect("begin_frame a");
    r1.render_frame(&scene_a, clear).expect("render a");
    r1.begin_frame(W, H, 1.0, 2).expect("begin_frame b");
    r1.render_frame(&scene_b, clear).expect("render b");
    let f1 = r1.read_rgba().expect("read_rgba f1");

    // Fresh renderer: B as a full repaint, the ground truth.
    let mut r2 = make().expect("second headless renderer");
    r2.begin_frame(W, H, 1.0, 1).expect("begin_frame full");
    r2.render_frame(&scene_b, clear).expect("render full");
    let full = r2.read_rgba().expect("read_rgba full");

    assert_eq!(f1.len(), full.len(), "readbacks differ in size");
    let mut diff_px = 0usize;
    for i in (0..f1.len()).step_by(4) {
        let d = (0..3)
            .map(|k| (f1[i + k] as i32 - full[i + k] as i32).abs())
            .max()
            .unwrap_or(0);
        if d > 16 {
            diff_px += 1;
        }
    }
    let total = (W * H) as usize;
    let frac = diff_px as f64 / total as f64;
    assert!(
        frac < 0.003,
        "F1 damage-prime output diverges from a full repaint: {diff_px}/{total} px differ by >16 ({:.3}%) — a ghosted or unrepainted cell",
        frac * 100.0
    );
}

// One LARGE semi-transparent rounded panel (which `expand_fill_layers` turns into a synthetic opacity
// layer spanning most of the surface) plus a small opaque indicator that changes between frames. The
// panel is much bigger than the indicator's dirty rect, so if F1 damage-tracks this frame the panel's
// unconfined layer composite double-applies opacity everywhere outside the dirty rect — the exact
// scenario F1 must skip. (A grid of small translucent cells does NOT reproduce it: cells outside the
// dirty rect are culled to empty layers whose composite is a no-op.)
fn translucent_panel_scene(indicator: Color) -> Vec<DrawCommand> {
    const W: f32 = 800.0;
    const H: f32 = 600.0;
    vec![
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, W, H),
            style: Arc::new(RectStyle::filled(Color::from_rgb_u8(20, 20, 28), 0.0)),
        },
        // radius > 0 + 0 < a < 1 + solid fill + no shadow → fill-layer expansion into a PushLayer.
        DrawCommand::Rect {
            rect: Rect::new(100.0, 80.0, 600.0, 440.0),
            style: Arc::new(RectStyle::filled(Color::rgba(0.4, 0.5, 0.7, 0.5), 20.0)),
        },
        // Small opaque indicator over the panel; only this changes between the two scenes.
        DrawCommand::Rect {
            rect: Rect::new(360.0, 280.0, 80.0, 40.0),
            style: Arc::new(RectStyle::filled(indicator, 4.0)),
        },
    ]
}

/// F1 damage tracking of a frame containing a fill-layer-expanded translucent panel must match a full
/// repaint — this exercises `confine_to_dirty`, which clips the panel's opacity composite to the dirty
/// rect. Without confinement the composite double-applies opacity outside the dirty rect and B-after-A
/// diverges from a full repaint of B by ~54% (measured); confined, it matches.
#[test]
fn damage_confines_translucent_layer_composite() {
    const W: u32 = 800;
    const H: u32 = 600;
    let clear = Some(Color::from_rgb_u8(20, 20, 28));
    let scene_a = translucent_panel_scene(Color::from_rgb_u8(40, 40, 50));
    let scene_b = translucent_panel_scene(Color::from_rgb_u8(240, 200, 60));

    let make = || {
        pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
            W,
            H,
            None,
            false,
            TextShaperConfig::default(),
            HardwareRendererConfig::default(),
        ))
    };

    let mut r1 = match make() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping translucent-layer damage test: no GPU adapter: {e:?}");
            return;
        }
    };
    r1.begin_frame(W, H, 1.0, 1).expect("begin a");
    r1.render_frame(&scene_a, clear).expect("render a");
    r1.begin_frame(W, H, 1.0, 2).expect("begin b");
    r1.render_frame(&scene_b, clear).expect("render b");
    let after = r1.read_rgba().expect("read after");

    let mut r2 = make().expect("second renderer");
    r2.begin_frame(W, H, 1.0, 1).expect("begin full");
    r2.render_frame(&scene_b, clear).expect("render full");
    let full = r2.read_rgba().expect("read full");

    let mut diff_px = 0usize;
    for i in (0..after.len()).step_by(4) {
        let d = (0..3)
            .map(|k| (after[i + k] as i32 - full[i + k] as i32).abs())
            .max()
            .unwrap_or(0);
        if d > 16 {
            diff_px += 1;
        }
    }
    let frac = diff_px as f64 / (W * H) as f64;
    assert!(
        frac < 0.003,
        "translucent-fill damage frame diverged from a full repaint: {diff_px} px differ ({:.3}%) — the \
         panel's opacity composite was not confined to the dirty rect (double-applied outside it)",
        frac * 100.0
    );
}

// A large explicit opacity PushLayer over the surface plus a small opaque indicator that changes. Same
// shape as the translucent-panel case but via a real `PushLayer{opacity}` rather than fill expansion.
fn opacity_layer_scene(indicator: Color) -> Vec<DrawCommand> {
    const W: f32 = 800.0;
    const H: f32 = 600.0;
    vec![
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, W, H),
            style: Arc::new(RectStyle::filled(Color::from_rgb_u8(20, 20, 28), 0.0)),
        },
        DrawCommand::PushLayer {
            opacity: 0.6,
            backdrop_blur: 0.0,
        },
        DrawCommand::Rect {
            rect: Rect::new(100.0, 80.0, 600.0, 440.0),
            style: Arc::new(RectStyle::filled(Color::from_rgb_u8(120, 180, 240), 0.0)),
        },
        DrawCommand::PopLayer,
        DrawCommand::Rect {
            rect: Rect::new(360.0, 280.0, 80.0, 40.0),
            style: Arc::new(RectStyle::filled(indicator, 4.0)),
        },
    ]
}

/// Same confinement check as the fill-layer test but through an explicit `PushLayer{opacity}`: the
/// layer spans far beyond the indicator's dirty rect, so its opacity composite must be clipped to the
/// dirty rect to match a full repaint.
#[test]
fn damage_confines_opacity_layer() {
    const W: u32 = 800;
    const H: u32 = 600;
    let clear = Some(Color::from_rgb_u8(20, 20, 28));
    let scene_a = opacity_layer_scene(Color::from_rgb_u8(40, 40, 50));
    let scene_b = opacity_layer_scene(Color::from_rgb_u8(240, 200, 60));

    let make = || {
        pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
            W,
            H,
            None,
            false,
            TextShaperConfig::default(),
            HardwareRendererConfig::default(),
        ))
    };
    let mut r1 = match make() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping opacity-layer damage test: no GPU adapter: {e:?}");
            return;
        }
    };
    r1.begin_frame(W, H, 1.0, 1).expect("begin a");
    r1.render_frame(&scene_a, clear).expect("render a");
    r1.begin_frame(W, H, 1.0, 2).expect("begin b");
    r1.render_frame(&scene_b, clear).expect("render b");
    let after = r1.read_rgba().expect("read after");

    let mut r2 = make().expect("second renderer");
    r2.begin_frame(W, H, 1.0, 1).expect("begin full");
    r2.render_frame(&scene_b, clear).expect("render full");
    let full = r2.read_rgba().expect("read full");

    let mut diff_px = 0usize;
    for i in (0..after.len()).step_by(4) {
        let d = (0..3)
            .map(|k| (after[i + k] as i32 - full[i + k] as i32).abs())
            .max()
            .unwrap_or(0);
        if d > 16 {
            diff_px += 1;
        }
    }
    let frac = diff_px as f64 / (W * H) as f64;
    assert!(
        frac < 0.003,
        "opacity-layer damage frame diverged from a full repaint: {diff_px} px differ ({:.3}%) — the \
         layer's composite was not confined to the dirty rect",
        frac * 100.0
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

/// Smoke test for the `render_frame` decomposition: renders a fixed scene headless and checks it succeeds
/// with a tightly-packed readback. It also hashes the pixels against a baked constant, but that hash is
/// GPU/platform-specific (see below), so it is enforced only under `RSX_HARDWARE_GOLDEN` on the baseline
/// machine — cross-platform CI relies on the smoke checks and the software renderer's deterministic golden.
#[test]
fn render_frame_pixel_golden() {
    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 800;
    // Baked from the pre-decomposition renderer on this machine's GPU; must not change post-split.
    const EXPECTED: u64 = 0x3507_0457_a257_bc16;

    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
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
    // GPU rasterisation (text hinting, antialiasing, subpixel coverage) is driver- and platform-specific,
    // so this exact hash only reproduces on the machine EXPECTED was baked on. Enforce it strictly only when
    // opted in (that machine); everywhere else — cross-platform CI — the assertions above (renders without
    // error, tightly-packed readback of the right size) are the portable smoke test, and a hash mismatch is
    // reported but not fatal. The deterministic cross-platform pixel guard is the software renderer's golden.
    if std::env::var_os("RSX_HARDWARE_GOLDEN").is_some() {
        assert_eq!(
            hash, EXPECTED,
            "golden pixel hash changed: render_frame behavior differs from baseline (got {hash:#018x})"
        );
    } else if hash != EXPECTED {
        eprintln!(
            "note: hardware golden hash {hash:#018x} != baseline {EXPECTED:#018x} \
             (expected on a different GPU/platform); set RSX_HARDWARE_GOLDEN=1 on the baseline machine to enforce"
        );
    }
}
