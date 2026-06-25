//! H2 + H6 from performance-research §8.
//!
//! H2 — the software COLR fallback's font-byte access. `font_data_for` re-reads/copies the (often
//! multi-MB) font on every call; `colr_font_bytes` returns an `Arc` clone after the first.
//! Requires a resolvable font; the bench skips cleanly when the host has none.
//!
//! H6 — the `collect_colr_glyphs` gating. Plain UI text is re-shaped (make_buffer + per-glyph swash
//! probe) on every emoji-fallback pass unless a cached `has_colr = false` flag short-circuits it.
//! The warm/gated speedup only materializes on a host whose fonts actually render the text (so the
//! glyphs are not flagged as COLR); without fonts both arms do the full work, but the bench still runs.

use std::cell::Cell;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use geometry_core::Rect;
use renderer_core::{Color, TextStyle};
use renderer_text::TextShaper;

fn ui_rect() -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: 240.0,
        height: 80.0,
    }
}

fn bench_font_bytes_h2(c: &mut Criterion) {
    let mut shaper = TextShaper::new();
    let Some(font_id) = shaper.default_face_id() else {
        eprintln!("H2 skipped: the host font system resolved no default face");
        return;
    };

    let mut group = c.benchmark_group("colr_font_bytes");
    // Before: read + copy the whole font on every access.
    group.bench_function("cold_font_data_for", |b| {
        b.iter(|| black_box(shaper.font_data_for(black_box(font_id))));
    });
    // After: warm the per-font cache once, then every access is an Arc clone.
    let _ = shaper.colr_font_bytes(font_id);
    group.bench_function("cached_arc_clone", |b| {
        b.iter(|| black_box(shaper.colr_font_bytes(black_box(font_id))));
    });
    group.finish();
}

fn bench_collect_colr_h6(c: &mut Criterion) {
    let style = TextStyle::new(16.0, Color::BLACK);
    let rect = ui_rect();

    let mut group = c.benchmark_group("collect_colr_glyphs");

    // Ungated: a unique text per iteration. More distinct texts than the flag cache cap, so each lands as a miss and pays the full make_buffer + per-glyph probe — the work the gate skips.
    let texts: Vec<String> = (0..2048).map(|i| format!("ui label number {i}")).collect();
    let mut shaper_cold = TextShaper::new();
    let idx = Cell::new(0usize);
    group.bench_function("ungated_full_shape", |b| {
        b.iter(|| {
            let i = idx.get();
            idx.set(i.wrapping_add(1) % texts.len());
            let mut out = Vec::new();
            shaper_cold.collect_colr_glyphs(black_box(&texts[i]), rect, &style, &mut out);
            black_box(out.len());
        });
    });

    // Gated: the same plain text every iteration. After the first call records the flag, later calls short-circuit (on a host whose fonts render the text) to a single hashmap probe.
    let mut shaper_warm = TextShaper::new();
    let warm_text = "ui label";
    {
        let mut out = Vec::new();
        shaper_warm.collect_colr_glyphs(warm_text, rect, &style, &mut out);
    }
    group.bench_function("gated_warm", |b| {
        b.iter(|| {
            let mut out = Vec::new();
            shaper_warm.collect_colr_glyphs(black_box(warm_text), rect, &style, &mut out);
            black_box(out.len());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_font_bytes_h2, bench_collect_colr_h6);
criterion_main!(benches);
