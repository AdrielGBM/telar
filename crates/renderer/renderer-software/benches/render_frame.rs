//! Baseline for the software backend's `render_frame` on a dense, HiDPI-sized UI frame, captured
//! before `render_frame` is decomposed. Runs fully headless (offscreen `Pixmap`, no window/softbuffer)
//! so it works in environments with only a GPU render node and no display server.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use geometry_core::{Point, Rect};
use platform_headless::HeadlessWindow;
use renderer_core::{
    BorderRadius, Color, DrawCommand, PathData, PathStyle, RectStyle, RenderBackend, Shadow,
    ShapeStyle, Stroke, TextStyle,
};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

/// A dense widget-tree frame: a grid of panels (fill + stroke border + shadow), labels, path icons
/// and separators, wrapped in a few opacity layers and a scroll-style clip. A handful of style/path
/// `Arc`s are reused across the whole list — the realistic case for a UI tree.
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
        rect: Rect::new(0.0, 0.0, WIDTH as f32, 48.0),
        style: panel_fill.clone(),
    });
    cmds.push(DrawCommand::Text {
        text: title.clone(),
        rect: Rect::new(16.0, 14.0, 400.0, 24.0),
        style: title_style.clone(),
    });
    cmds.push(DrawCommand::Line {
        p1: Point::new(0.0, 48.0),
        p2: Point::new(WIDTH as f32, 48.0),
        style: separator,
    });
    cmds.push(DrawCommand::PopLayer);

    // Scrollable content region: a clip plus a shift matrix, then a grid of cards.
    cmds.push(DrawCommand::PushClip {
        rect: Rect::new(0.0, 56.0, WIDTH as f32, HEIGHT as f32 - 56.0),
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

fn bench_render_frame(c: &mut Criterion) {
    let cmds = dense_ui();
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        WIDTH,
        HEIGHT,
        SoftwareRendererConfig::default(),
    );

    // Smoke test: a headless render must land visible pixels in the pixmap (not empty, not all-zero).
    renderer.begin_frame(WIDTH, HEIGHT, 1.0, 0).unwrap();
    renderer
        .render_frame(&cmds, Some(Color::from_rgb_u8(15, 16, 22)))
        .unwrap();
    let rgba = renderer
        .read_rgba()
        .expect("headless pixmap should exist after a frame");
    assert_eq!(rgba.len(), (WIDTH * HEIGHT * 4) as usize);
    assert!(
        rgba.iter().any(|&b| b != 0),
        "frame rendered but pixmap is all zero"
    );

    let mut group = c.benchmark_group("render_frame_sw");
    group.bench_function("dense_ui", |b| {
        // Toggle the clear color every iteration so render_frame's skip-if-unchanged fast path never short-circuits the benchmark; each iteration renders the full scene.
        let mut toggle = false;
        b.iter(|| {
            let clear = if toggle {
                Color::from_rgb_u8(15, 16, 22)
            } else {
                Color::from_rgb_u8(16, 17, 23)
            };
            toggle = !toggle;
            renderer.begin_frame(WIDTH, HEIGHT, 1.0, 0).unwrap();
            renderer
                .render_frame(black_box(&cmds), Some(clear))
                .unwrap();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_render_frame);
criterion_main!(benches);
