//! H1 (highest-priority benchmark from performance-research §8): scaling a dense, HiDPI software
//! frame. Compares the previous behaviour — a fresh `Vec` plus an `Arc::new` for every styled
//! command, every frame — against the reusable `ScaleScratch` (buffer reuse + per-frame
//! pointer-keyed Arc cache, so styles shared across the command list are scaled once).

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use geometry_core::Rect;
use renderer_core::{
    BorderRadius, Color, DrawCommand, RectStyle, ScaleScratch, TextStyle, scale_commands,
};

/// A dense UI frame: many commands, but only a handful of distinct style `Arc`s reused across them —
/// the realistic case for a widget tree, and exactly what the pointer-keyed Arc cache exploits.
fn dense_ui(n: usize) -> Vec<DrawCommand> {
    let rect_styles: Vec<Arc<RectStyle>> = (0..8)
        .map(|i| Arc::new(RectStyle::default().with_radius(BorderRadius::all(4.0 + i as f32))))
        .collect();
    let text_styles: Vec<Arc<TextStyle>> = (0..8)
        .map(|i| Arc::new(TextStyle::new(12.0 + i as f32, Color::BLACK)))
        .collect();

    let mut cmds = Vec::with_capacity(n * 3);
    for i in 0..n {
        let nested = i % 4 == 0;
        if nested {
            cmds.push(DrawCommand::PushMatrix {
                matrix: [1.0, 0.0, 0.0, 1.0, i as f32, (i * 2) as f32],
            });
        }
        cmds.push(DrawCommand::Rect {
            rect: Rect::new(i as f32, i as f32, 100.0, 40.0),
            style: rect_styles[i % rect_styles.len()].clone(),
        });
        cmds.push(DrawCommand::Text {
            text: Arc::from("label"),
            rect: Rect::new(i as f32, i as f32, 80.0, 20.0),
            style: text_styles[i % text_styles.len()].clone(),
        });
        if nested {
            cmds.push(DrawCommand::PopMatrix);
        }
    }
    cmds
}

fn bench_scale_commands(c: &mut Criterion) {
    let cmds = dense_ui(400);
    let sf = 3.0;
    let mut group = c.benchmark_group("scale_commands");

    group.bench_function("baseline_vec_per_frame", |b| {
        b.iter(|| {
            let out = scale_commands(black_box(&cmds), black_box(sf));
            black_box(out);
        });
    });

    let mut scratch = ScaleScratch::new();
    group.bench_function("scale_scratch_reused", |b| {
        b.iter(|| {
            let out = scratch.scale_into(black_box(&cmds), black_box(sf));
            black_box(out.len());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_scale_commands);
criterion_main!(benches);
