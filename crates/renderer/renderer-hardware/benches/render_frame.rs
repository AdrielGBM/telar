//! Baseline benchmark for the hardware `render_frame` before its planned decomposition. Builds a headless (offscreen, no window/surface) `HardwareRenderer` and times a single representative "dense UI" frame: filled+shadowed rounded cards, text, vector paths, and a translucent overlay layer with a backdrop blur. Each iteration blocks on the GPU (`wait_idle`) so the measurement reflects real render work, not just queue submission.
//!
//! Requires a usable GPU adapter; if headless init fails (no GPU in the environment) the bench prints the error and skips rather than reporting fabricated numbers.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use geometry_core::{Point, Rect};
use platform_headless::HeadlessWindow;
use renderer_core::{
    Color, DrawCommand, PathData, PathStyle, RectStyle, RenderBackend, Shadow, ShapeStyle,
    TextStyle,
};
use renderer_text::TextShaperConfig;
use telar_renderer_hardware::HardwareRenderer;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

/// A representative dense frame exercising every major primitive path plus a backdrop-blur layer.
fn dense_ui() -> Vec<DrawCommand> {
    let mut cmds = Vec::new();

    cmds.push(DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32),
        style: Arc::new(RectStyle::filled(Color::from_rgb_u8(24, 24, 32), 0.0)),
    });

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
    let cell_w = (WIDTH as f32 - pad) / cols as f32;
    let cell_h = ((HEIGHT as f32 - pad) * 0.85) / rows as f32;
    for r in 0..rows {
        for col in 0..cols {
            let x = pad + col as f32 * cell_w;
            let y = pad + r as f32 * cell_h;
            cmds.push(DrawCommand::Rect {
                rect: Rect::new(x, y, cell_w - pad, cell_h - pad),
                style: card_style.clone(),
            });
            cmds.push(DrawCommand::Text {
                spans: None,
                text: Arc::from("Widget label"),
                rect: Rect::new(x + 12.0, y + 12.0, cell_w - pad - 24.0, 20.0),
                style: text_style.clone(),
            });
        }
    }

    let path_style = Arc::new(PathStyle::default().with_fill(Color::from_rgb_u8(120, 200, 255)));
    for i in 0..24 {
        let cx = 40.0 + i as f32 * 50.0;
        let path = PathData::polygon(&[
            Point::new(cx, HEIGHT as f32 - 70.0),
            Point::new(cx + 22.0, HEIGHT as f32 - 24.0),
            Point::new(cx - 22.0, HEIGHT as f32 - 24.0),
        ]);
        cmds.push(DrawCommand::Path {
            data: Arc::new(path),
            style: path_style.clone(),
        });
    }

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
            spans: None,
            text: Arc::from("Overlay content line"),
            rect: Rect::new(360.0, 280.0 + i as f32 * 32.0, 560.0, 24.0),
            style: text_style.clone(),
        });
    }
    cmds.push(DrawCommand::PopLayer);

    cmds
}

fn bench_render_frame(c: &mut Criterion) {
    let mut renderer = match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
        WIDTH,
        HEIGHT,
        None,
        false,
        TextShaperConfig::default(),
    )) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping render_frame benchmark: headless GPU init failed: {e:?}");
            return;
        }
    };

    let cmds = dense_ui();
    let clear = Some(Color::from_rgb_u8(18, 18, 24));

    // Fixed generation: headless always takes the full render path (no surface to idle-blit into), so the frame is fully rendered every iteration regardless.
    let generation = 1u64;

    let mut group = c.benchmark_group("render_frame_hw");
    group.bench_function("dense_ui", |b| {
        b.iter(|| {
            renderer
                .begin_frame(WIDTH, HEIGHT, 1.0, generation)
                .expect("begin_frame failed");
            renderer
                .render_frame(black_box(&cmds), black_box(clear))
                .expect("render_frame failed");
            // Block on the submitted GPU work so the timing reflects real render cost.
            renderer.wait_idle().expect("wait_idle failed");
        });
    });
    group.finish();
}

criterion_group!(benches, bench_render_frame);
criterion_main!(benches);
