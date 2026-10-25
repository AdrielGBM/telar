//! What carrying a frame across the guest boundary as bytes costs, next to what building that frame costs.
//!
//! The guest encodes each frame into a buffer it owns and the host decodes it into commands of its own (see `renderer_core::wire`). Two frames alternate, as a hover does, so `steady` is what consecutive frames pay once their shared payloads are interned; `cold` resets the encoder every frame, which is what the first frame and a frame whose payloads are all new pay. `build` is the frame the guest makes, and `clone` what crossing by value costs today.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use platform_core::PointerSource;
use renderer_core::wire::{FrameDecoder, FrameEncoder};
use renderer_core::{
    Color, DrawCommand, Element, ElementId, RectStyle, Role, Semantics, ShapeStyle, TextStyle,
};
use telar::{App, Component, Event, Rect, RenderNode, RwSignal, signal};

const WINDOW_W: u32 = 1200;
const WINDOW_H: u32 = 900;
const SYNTHETIC_ROWS: usize = 5000;

#[derive(Default)]
struct Codec {
    encoder: FrameEncoder,
    decoder: FrameDecoder,
    out: Vec<DrawCommand>,
}

impl Codec {
    fn run(&mut self, iters: u64, frames: &[Vec<DrawCommand>; 2], cold: bool) -> [Duration; 2] {
        let mut spent = [Duration::ZERO; 2];
        for iter in 0..iters {
            if cold {
                self.encoder.reset();
            }
            let start = Instant::now();
            black_box(self.encoder.encode(&frames[iter as usize % 2]));
            spent[0] += start.elapsed();
            let start = Instant::now();
            self.decoder
                .decode(self.encoder.bytes(), &mut self.out, |id| {
                    self.encoder.pixels(id).map(<[u8]>::to_vec)
                })
                .expect("the frame decodes");
            spent[1] += start.elapsed();
            black_box(&self.out);
        }
        spent
    }
}

fn bench_codec(c: &mut Criterion, name: &str, frames: &[Vec<DrawCommand>; 2]) {
    report(name, frames);
    let mut group = c.benchmark_group(format!("boundary/{name}"));
    group.bench_function("clone", |b| b.iter(|| black_box(frames[0].clone())));
    for (label, cold) in [("steady", false), ("cold", true)] {
        let mut codec = Codec::default();
        group.bench_function(format!("encode_{label}"), |b| {
            b.iter_custom(|iters| codec.run(iters, frames, cold)[0])
        });
        let mut codec = Codec::default();
        group.bench_function(format!("decode_{label}"), |b| {
            b.iter_custom(|iters| codec.run(iters, frames, cold)[1])
        });
    }
    group.finish();
}

fn report(name: &str, frames: &[Vec<DrawCommand>; 2]) {
    let mut encoder = FrameEncoder::new();
    let cold = encoder.encode(&frames[0]).len();
    let steady = encoder.encode(&frames[1]).len();
    eprintln!(
        "\n--- {name}: {} commands, {cold} bytes cold, {steady} bytes steady ---\n",
        frames[0].len()
    );
}

fn hover(y: f64) -> Event {
    Event::PointerMoved {
        x: 110.0,
        y,
        source: PointerSource::Mouse,
    }
}

fn bench_sandbox(c: &mut Criterion) {
    renderer_core::set_text_metrics(renderer_text::ShaperMetrics);
    telar::set_theme(sandbox::core::theme::SandboxTheme::modern());
    let mut tree = telar::ComponentList::new(sandbox::core::app::SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: WINDOW_W,
        height: WINDOW_H,
    });
    let mut frame_at = |y| {
        tree.on_event(&hover(y));
        tree.commands().clone()
    };
    let frames = [frame_at(453.0), frame_at(489.0)];
    assert_ne!(frames[0], frames[1], "the hover changes the frame");

    let mut group = c.benchmark_group("boundary/sandbox_1200x900");
    group.bench_function("build", |b| {
        let mut toggle = false;
        b.iter(|| {
            toggle = !toggle;
            tree.on_event(&hover(if toggle { 489.0 } else { 453.0 }));
            black_box(tree.commands().len());
        });
    });
    group.finish();
    bench_codec(c, "sandbox_1200x900", &frames);
}

/// Every row a box with its own label around a rect and a run of text in a style of its own, all rebuilt on each tick: nothing a frame draws is shared with the frame before, so every payload crosses again.
struct Synthetic {
    tick: RwSignal<u32>,
}

impl Component for Synthetic {
    fn view(&self) -> RenderNode {
        let tick = self.tick.get();
        RenderNode::group((0..SYNTHETIC_ROWS).map(|row| {
            let y = row as f32 * 18.0;
            let label = format!("Row {row} at tick {tick}");
            let semantics = Semantics::of(Role::Button).with_label(label.as_str());
            let element = Element::new(
                ElementId(row as u64),
                semantics,
                "",
                Rect::new(0.0, y, 400.0, 18.0),
            );
            let style = TextStyle::new(11.0 + (row % 7) as f32, Color::BLACK)
                .with_font_weight(400 + (row % 3) as u16 * 100)
                .with_letter_spacing(tick as f32 * 0.001);
            RenderNode::element(
                Arc::new(element),
                [
                    RenderNode::rect(
                        Rect::new(0.0, y, 400.0, 18.0),
                        RectStyle::default().with_fill(Color::WHITE),
                    ),
                    RenderNode::text(label, Rect::new(4.0, y, 392.0, 18.0), style),
                ],
            )
        }))
    }
}

fn bench_synthetic(c: &mut Criterion) {
    let tick = signal(0u32);
    let tree = telar::ComponentList::new(Synthetic { tick });
    let frame_at = |at| {
        tick.set(at);
        tree.commands().clone()
    };
    let frames = [frame_at(1), frame_at(2)];

    let name = format!("synthetic_{SYNTHETIC_ROWS}_rows");
    let mut group = c.benchmark_group(format!("boundary/{name}"));
    group.bench_function("build", |b| {
        let mut at = 2;
        b.iter(|| {
            at += 1;
            tick.set(at);
            black_box(tree.commands().len());
        });
    });
    group.finish();
    bench_codec(c, &name, &frames);
}

criterion_group!(benches, bench_sandbox, bench_synthetic);
criterion_main!(benches);
