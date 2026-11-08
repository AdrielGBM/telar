use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use platform_core::PointerSource;
use renderer_core::{
    Color, DrawCommand, Element, ElementId, RectStyle, Role, Semantics, ShapeStyle, TextStyle,
};
use telar::{App, Component, Event, Rect, RenderNode, RwSignal, signal};

#[path = "../../catalogue/benches/support/codec_harness.rs"]
mod codec_harness;

use codec_harness::{bench_codec, report};

const WINDOW_W: u32 = 1200;
const WINDOW_H: u32 = 900;
const SYNTHETIC_ROWS: usize = 5000;

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

    let name = format!("sandbox_{WINDOW_W}x{WINDOW_H}");
    report(&name, &frames);
    let mut group = c.benchmark_group(format!("boundary/{name}"));
    group.bench_function("build", |b| {
        let mut toggle = false;
        b.iter(|| {
            toggle = !toggle;
            tree.on_event(&hover(if toggle { 489.0 } else { 453.0 }));
            black_box(tree.commands().len());
        });
    });
    bench_codec(&mut group, &frames);
    group.finish();
}

/// Each row is rebuilt on every tick with new payloads, so no payload is shared with the previous frame and every one crosses again.
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
    let frames: [Vec<DrawCommand>; 2] = [frame_at(1), frame_at(2)];

    let name = format!("synthetic_{SYNTHETIC_ROWS}_rows");
    report(&name, &frames);
    let mut group = c.benchmark_group(format!("boundary/{name}"));
    group.bench_function("build", |b| {
        let mut at = 2;
        b.iter(|| {
            at += 1;
            tick.set(at);
            black_box(tree.commands().len());
        });
    });
    bench_codec(&mut group, &frames);
    group.finish();
}

criterion_group!(benches, bench_sandbox, bench_synthetic);
criterion_main!(benches);
