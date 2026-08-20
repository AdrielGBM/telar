use std::sync::Arc;

use geometry_core::{Point, Rect};
use platform_headless::HeadlessWindow;
use renderer_core::{
    BorderRadius, Color, DrawCommand, PathData, PathStyle, RectStyle, RenderBackend, Shadow,
    ShapeStyle, Stroke, TextStyle,
};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

#[test]
fn headless_renders_visible_pixels() {
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        64,
        48,
        SoftwareRendererConfig::default(),
    );

    let cmds = vec![
        DrawCommand::Rect {
            rect: Rect::new(8.0, 8.0, 40.0, 24.0),
            style: Arc::new(RectStyle::default().with_fill(Color::from_rgb_u8(200, 60, 60))),
        },
        DrawCommand::Text {
            text: Arc::from("hi"),
            rect: Rect::new(10.0, 10.0, 40.0, 16.0),
            style: Arc::new(TextStyle::new(12.0, Color::WHITE)),
        },
    ];

    renderer.begin_frame(64, 48, 1.0, 0).unwrap();
    renderer
        .render_frame(&cmds, Some(Color::from_rgb_u8(10, 10, 10)))
        .unwrap();

    let rgba = renderer.read_rgba().expect("pixmap exists after a frame");
    assert_eq!(rgba.len(), 64 * 48 * 4);
    // Something was drawn: at least one pixel differs from the clear color's red channel.
    assert!(
        rgba.chunks_exact(4).any(|px| px[0] != 10),
        "expected drawn content to differ from the clear color"
    );
    assert!(renderer.pixmap().is_some());
}

#[test]
fn headless_present_is_noop_without_surface() {
    // A second frame with unchanged commands exercises the skip-if-unchanged path, which calls the present no-op.
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        32,
        32,
        SoftwareRendererConfig::default(),
    );
    let cmds = vec![DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, 32.0, 32.0),
        style: Arc::new(RectStyle::default().with_fill(Color::BLUE)),
    }];
    renderer.begin_frame(32, 32, 1.0, 0).unwrap();
    renderer.render_frame(&cmds, Some(Color::BLACK)).unwrap();
    // Same commands + same clear color: hits the fast path and the surface-less present.
    renderer.render_frame(&cmds, Some(Color::BLACK)).unwrap();
    assert!(renderer.read_rgba().unwrap().iter().any(|&b| b != 0));
}

// A transparent app clears to a translucent color; the readback must keep that alpha (premultiplied) rather
// than forcing the surface opaque. This is the same pixmap→bytes path the Wayland ARGB8888 present reads, so
// it verifies transparency preservation end to end for the software backend without a compositor.
#[test]
fn headless_preserves_translucent_clear_alpha() {
    let config = SoftwareRendererConfig {
        transparent: true,
        ..Default::default()
    };
    let mut renderer =
        SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(16, 16, config);
    renderer.begin_frame(16, 16, 1.0, 0).unwrap();
    // The dim scrim the drawer uses: black at ~35% alpha.
    renderer
        .render_frame(&[], Some(Color::from_rgba_u8(0, 0, 0, 90)))
        .unwrap();
    let rgba = renderer.read_rgba().expect("pixmap exists after a frame");
    // Every pixel is the premultiplied scrim: RGB 0, alpha 90 — the alpha is NOT clamped to opaque.
    assert!(
        rgba.chunks_exact(4).all(|px| px == [0, 0, 0, 90]),
        "translucent clear must preserve alpha; got first px {:?}",
        &rgba[0..4]
    );
}

const GOLDEN_WIDTH: u32 = 1280;
const GOLDEN_HEIGHT: u32 = 800;

// Mirror of the `dense_ui` scene from `benches/render_frame.rs`: a dense widget tree exercising
// fills, strokes, inline shadows, text, path icons, separators, opacity layers and a scroll clip.
// Kept in sync manually — it is a fixed, representative frame used as a byte-exact render golden.
fn dense_ui() -> Vec<DrawCommand> {
    let panel_fill: Arc<RectStyle> = Arc::new(
        RectStyle::default()
            .with_fill(Color::from_rgb_u8(30, 34, 44))
            .with_radius(BorderRadius::all(6.0)),
    );
    let panel_border: Arc<RectStyle> = Arc::new(
        RectStyle::default()
            .with_stroke(Stroke::new(Color::from_rgb_u8(70, 78, 96), 1.5))
            .with_radius(BorderRadius::all(6.0)),
    );
    // Small blur/spread so the shadow pixmap stays under the async threshold and is computed inline (deterministic, no background threads).
    let card_shadow: Arc<RectStyle> = Arc::new(
        RectStyle::default()
            .with_fill(Color::from_rgb_u8(24, 27, 35))
            .with_radius(BorderRadius::all(8.0))
            .with_shadow(Shadow::new(0.0, 2.0, 6.0, Color::rgba(0.0, 0.0, 0.0, 0.45))),
    );
    let label_style: Arc<TextStyle> =
        Arc::new(TextStyle::new(13.0, Color::from_rgb_u8(220, 224, 232)));
    let title_style: Arc<TextStyle> = Arc::new(TextStyle::new(16.0, Color::WHITE));

    let icon: Arc<PathData> = Arc::new(
        PathData::new()
            .move_to(Point::new(0.0, 8.0))
            .line_to(Point::new(6.0, 14.0))
            .line_to(Point::new(16.0, 0.0))
            .quad_to(Point::new(10.0, 6.0), Point::new(6.0, 10.0))
            .close(),
    );
    let icon_style: Arc<PathStyle> = Arc::new(
        PathStyle::default()
            .with_fill(Color::from_rgb_u8(90, 200, 140))
            .with_stroke(Stroke::new(Color::from_rgb_u8(40, 120, 80), 1.0)),
    );

    let separator = Stroke::new(Color::from_rgb_u8(56, 62, 78), 1.0);
    let label: Arc<str> = Arc::from("Item label");
    let title: Arc<str> = Arc::from("Section title");

    let mut cmds = Vec::with_capacity(2048);

    // Top bar as an opacity layer with its own title text and separator line.
    cmds.push(DrawCommand::PushLayer {
        opacity: 0.96,
        backdrop_blur: 0.0,
    });
    cmds.push(DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, GOLDEN_WIDTH as f32, 48.0),
        style: panel_fill.clone(),
    });
    cmds.push(DrawCommand::Text {
        text: title.clone(),
        rect: Rect::new(16.0, 14.0, 400.0, 24.0),
        style: title_style.clone(),
    });
    cmds.push(DrawCommand::Line {
        p1: Point::new(0.0, 48.0),
        p2: Point::new(GOLDEN_WIDTH as f32, 48.0),
        style: separator,
    });
    cmds.push(DrawCommand::PopLayer);

    // Scrollable content region: a clip plus a shift matrix, then a grid of cards.
    cmds.push(DrawCommand::PushClip {
        rect: Rect::new(0.0, 56.0, GOLDEN_WIDTH as f32, GOLDEN_HEIGHT as f32 - 56.0),
        radius: BorderRadius::zero(),
    });
    cmds.push(DrawCommand::PushMatrix {
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 56.0],
    });

    let cols = 5;
    let rows = 6;
    let cell_w = 236.0;
    let cell_h = 116.0;
    let gap = 16.0;
    for row in 0..rows {
        for col in 0..cols {
            let x = 16.0 + col as f32 * (cell_w + gap);
            let y = 8.0 + row as f32 * (cell_h + gap);
            let card = Rect::new(x, y, cell_w, cell_h);

            // Shadowed card background.
            cmds.push(DrawCommand::Rect {
                rect: card,
                style: card_shadow.clone(),
            });
            // Inner panel with a nested opacity layer to exercise layer compositing on some cards.
            let layered = (row + col) % 3 == 0;
            if layered {
                cmds.push(DrawCommand::PushLayer {
                    opacity: 0.85,
                    backdrop_blur: 0.0,
                });
            }
            cmds.push(DrawCommand::Rect {
                rect: Rect::new(x + 6.0, y + 6.0, cell_w - 12.0, cell_h - 12.0),
                style: panel_fill.clone(),
            });
            cmds.push(DrawCommand::Rect {
                rect: Rect::new(x + 6.0, y + 6.0, cell_w - 12.0, cell_h - 12.0),
                style: panel_border.clone(),
            });
            // Icon.
            cmds.push(DrawCommand::PushMatrix {
                matrix: [1.0, 0.0, 0.0, 1.0, x + 16.0, y + 16.0],
            });
            cmds.push(DrawCommand::Path {
                data: icon.clone(),
                style: icon_style.clone(),
            });
            cmds.push(DrawCommand::PopMatrix);
            // Two text lines.
            cmds.push(DrawCommand::Text {
                text: label.clone(),
                rect: Rect::new(x + 40.0, y + 16.0, cell_w - 56.0, 20.0),
                style: label_style.clone(),
            });
            cmds.push(DrawCommand::Text {
                text: label.clone(),
                rect: Rect::new(x + 16.0, y + 64.0, cell_w - 32.0, 20.0),
                style: label_style.clone(),
            });
            // Divider.
            cmds.push(DrawCommand::Line {
                p1: Point::new(x + 16.0, y + 48.0),
                p2: Point::new(x + cell_w - 16.0, y + 48.0),
                style: separator,
            });
            if layered {
                cmds.push(DrawCommand::PopLayer);
            }
        }
    }

    cmds.push(DrawCommand::PopMatrix);
    cmds.push(DrawCommand::PopClip);
    cmds
}

// Stable, version-independent FNV-1a fold over the frame bytes, so the golden constant does not
// depend on any hasher's internal representation.
fn fold_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// Render golden for the `render_frame` decomposition: renders the fixed `dense_ui` scene headless and
// folds the RGBA into a stable hash, which must stay identical across any behavior-preserving refactor.
//
// The hash is NOT a cross-platform CI gate, for two reasons that both flip it per machine without any
// behavior change: (1) the scene's text has no bundled font, so cosmic-text falls back to the host's
// system fonts — a different typeface on Linux vs macOS vs Windows; (2) tiny-skia is built with `simd`,
// whose float rounding can differ between x86 (Linux/Windows runners) and ARM (Apple-Silicon macOS
// runners). So enforce the exact hash only under `TELAR_SOFTWARE_GOLDEN` on the baseline machine; elsewhere
// the always-on checks below (renders without error, tightly-packed readback of the right size, content
// actually drawn) are the portable smoke test and a hash mismatch is reported, not fatal. Mirrors the
// hardware golden (`renderer-hardware/tests/headless_smoke.rs`, gated by `TELAR_HARDWARE_GOLDEN`).
#[test]
fn render_frame_pixel_golden() {
    const EXPECTED: u64 = 0x165a_2776_436b_d755;

    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        GOLDEN_WIDTH,
        GOLDEN_HEIGHT,
        SoftwareRendererConfig::default(),
    );
    let cmds = dense_ui();

    renderer
        .begin_frame(GOLDEN_WIDTH, GOLDEN_HEIGHT, 1.0, 0)
        .unwrap();
    // Clear to (15,16,22); cleared pixels read back with R == 15.
    renderer
        .render_frame(&cmds, Some(Color::from_rgb_u8(15, 16, 22)))
        .unwrap();

    let rgba = renderer
        .read_rgba()
        .expect("headless pixmap should exist after a frame");
    // Portable smoke checks (always on): tightly-packed readback of the right size, with content drawn.
    assert_eq!(rgba.len(), (GOLDEN_WIDTH * GOLDEN_HEIGHT * 4) as usize);
    assert!(
        rgba.chunks_exact(4).any(|px| px[0] != 15),
        "expected the scene to draw content differing from the clear color"
    );

    let hash = fold_bytes(rgba);
    if std::env::var_os("TELAR_SOFTWARE_GOLDEN").is_some() {
        assert_eq!(
            hash, EXPECTED,
            "render_frame pixel output changed: got {hash:#018x}, expected {EXPECTED:#018x}"
        );
    } else if hash != EXPECTED {
        eprintln!(
            "note: software golden hash {hash:#018x} != baseline {EXPECTED:#018x} \
             (expected on a different platform/arch or system fonts); \
             set TELAR_SOFTWARE_GOLDEN=1 on the baseline machine to enforce"
        );
    }
}

// A clip emitted UNDER an active matrix (the `object-fit: cover` / scrolled-widget case) must move
// with that matrix: PushMatrix{100,80} then PushClip{(0,0,40,30)} clips to window (100,80,40,30),
// so a fill covering the whole window only survives inside that transformed box.
#[test]
fn clip_composes_with_active_matrix() {
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        200,
        160,
        SoftwareRendererConfig::default(),
    );
    let red = Arc::new(RectStyle::default().with_fill(Color::from_rgb_u8(220, 40, 40)));
    let cmds = vec![
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 100.0, 80.0],
        },
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 40.0, 30.0),
            radius: BorderRadius::zero(),
        },
        // Local-space rect spanning the whole window (window 0,0..200,160) so only the clip crops it.
        DrawCommand::Rect {
            rect: Rect::new(-100.0, -80.0, 400.0, 400.0),
            style: red.clone(),
        },
        DrawCommand::PopClip,
        DrawCommand::PopMatrix,
    ];
    renderer.begin_frame(200, 160, 1.0, 0).unwrap();
    renderer
        .render_frame(&cmds, Some(Color::from_rgb_u8(0, 0, 0)))
        .unwrap();
    let rgba = renderer.read_rgba().unwrap();
    let red_at = |x: u32, y: u32| rgba[((y * 200 + x) * 4) as usize];

    // Inside the transformed clip window (100..140, 80..110): red.
    assert!(
        red_at(120, 95) > 150,
        "inside the transformed clip should be red, got r={}",
        red_at(120, 95)
    );
    // Outside the transformed clip: clear (this is exactly what regressed as "cover shows nothing").
    assert!(
        red_at(20, 20) < 60,
        "above/left of the clip should be clear, got r={}",
        red_at(20, 20)
    );
    assert!(
        red_at(150, 95) < 60,
        "just past the clip's right edge should be clear, got r={}",
        red_at(150, 95)
    );
    assert!(
        red_at(170, 140) < 60,
        "below/right of the clip should be clear, got r={}",
        red_at(170, 140)
    );
}

// The cross-platform pixel guard the two hash goldens cannot be. Both of those enforce their hash only
// under an env var, for reasons that hold: system-font fallback and tiny-skia's SIMD float rounding both
// flip a hash per machine without any behaviour change. This scene removes both variables instead of
// tolerating them — no text, and every edge on an integer boundary, so no pixel ever carries partial
// coverage and there is nothing for float rounding to disagree about. What survives is exact everywhere,
// which is what makes it safe to assert unconditionally.
//
// It pins the parts of `render_frame` a refactor of the command loop actually moves: clip intersection,
// matrix translation, layer compositing, and paint order.
#[test]
fn axis_aligned_scene_is_pixel_exact_on_every_platform() {
    const W: u32 = 64;
    const H: u32 = 48;
    let clear = Color::from_rgb_u8(0, 0, 0);
    let red = Color::from_rgb_u8(255, 0, 0);
    let green = Color::from_rgb_u8(0, 255, 0);
    let blue = Color::from_rgb_u8(0, 0, 255);

    let solid = |c: Color| Arc::new(RectStyle::default().with_fill(c));
    let cmds = vec![
        // Painted first, then covered on its right half — pins paint order.
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 32.0, 16.0),
            style: solid(red),
        },
        DrawCommand::Rect {
            rect: Rect::new(16.0, 0.0, 16.0, 16.0),
            style: solid(green),
        },
        // Clipped: only the left half of the blue rect may survive.
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 16.0, 16.0, 16.0),
            radius: BorderRadius::all(0.0),
        },
        DrawCommand::Rect {
            rect: Rect::new(0.0, 16.0, 32.0, 16.0),
            style: solid(blue),
        },
        DrawCommand::PopClip,
        // Translated: drawn at x=0 under a +32 matrix, so it must land at x=32.
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 32.0, 32.0],
        },
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 16.0, 16.0),
            style: solid(green),
        },
        DrawCommand::PopMatrix,
    ];

    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        W,
        H,
        SoftwareRendererConfig::default(),
    );
    renderer.begin_frame(W, H, 1.0, 0).unwrap();
    renderer.render_frame(&cmds, Some(clear)).unwrap();
    let rgba = renderer.read_rgba().expect("pixmap after a frame");

    let px = |x: u32, y: u32| -> (u8, u8, u8) {
        let i = ((y * W + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2])
    };

    assert_eq!(
        px(8, 8),
        (255, 0, 0),
        "the uncovered half of the first rect"
    );
    assert_eq!(
        px(24, 8),
        (0, 255, 0),
        "the later rect paints over the earlier one"
    );
    assert_eq!(px(8, 24), (0, 0, 255), "inside the clip");
    assert_eq!(
        px(24, 24),
        (0, 0, 0),
        "outside the clip, so still the clear colour"
    );
    assert_eq!(
        px(40, 40),
        (0, 255, 0),
        "the matrix moved this rect to x=32"
    );
    assert_eq!(
        px(8, 40),
        (0, 0, 0),
        "and left nothing behind at its untranslated origin"
    );
}

/// Text inside a matrix that scales comes out at the scaled size — once, not twice.
///
/// This backend hands the matrix to the rasteriser, so the glyphs are transformed with everything
/// else and nothing extra is needed. The hardware one only moves the box and lays the glyphs out
/// itself, so there it has to be told. The bound below is what says the two agree: ink grows with
/// the *area*, so twice the scale is about four times the ink. Scaling both the layout and the
/// raster would be about sixteen.
#[test]
fn text_under_a_scaled_matrix_is_drawn_at_the_scaled_size_once() {
    let inked = |matrix: Option<[f32; 6]>| {
        let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
            256,
            128,
            SoftwareRendererConfig::default(),
        );

        let mut cmds = Vec::new();
        if let Some(matrix) = matrix {
            cmds.push(DrawCommand::PushMatrix { matrix });
        }
        cmds.push(DrawCommand::Text {
            text: Arc::from("MMM"),
            rect: Rect::new(2.0, 2.0, 60.0, 20.0),
            style: Arc::new(TextStyle::new(8.0, Color::WHITE)),
        });
        if matrix.is_some() {
            cmds.push(DrawCommand::PopMatrix);
        }

        renderer.begin_frame(256, 128, 1.0, 0).unwrap();
        renderer.render_frame(&cmds, Some(Color::BLACK)).unwrap();
        let rgba = renderer.read_rgba().expect("a frame was drawn");
        rgba.chunks_exact(4).filter(|px| px[0] > 40).count()
    };

    let plain = inked(None);
    let doubled = inked(Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));

    assert!(plain > 0, "the text was drawn at all");
    assert!(
        doubled > plain * 2,
        "at twice the scale the letters got bigger: {plain} then {doubled}"
    );
    assert!(
        doubled < plain * 8,
        "and only once — this looks like the size was scaled and then the raster too: \
         {plain} then {doubled}"
    );

    // And a translation is not a scale, so text that is only moved is the size it was.
    let moved = inked(Some([1.0, 0.0, 0.0, 1.0, 3.0, 3.0]));
    assert!(
        moved.abs_diff(plain) < plain / 4,
        "moving it changed its size: {plain} then {moved}"
    );
}
