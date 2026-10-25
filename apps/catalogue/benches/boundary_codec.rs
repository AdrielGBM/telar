//! What carrying a workshop frame across the guest boundary as bytes costs, next to what building that frame costs: the workshop over every preview the catalogue shows, the largest frame a guest in this workspace draws.
//!
//! Measured as `apps/sandbox/benches/boundary_codec.rs` measures the sandbox: two frames alternate as a hover moves between two rows of the sidebar, `steady` encodes each on the last, `cold` resets the encoder every frame, and `clone` is what crossing by value costs today.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use renderer_core::wire::{FrameDecoder, FrameEncoder};
use renderer_core::{DrawCommand, for_each_with_matrix, transform_clip_rect};
use telar::{App, ComponentList, Event, PointerSource, Size};

const WINDOW_W: u32 = 1400;
const WINDOW_H: u32 = 900;

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

fn hover((x, y): (f64, f64)) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

/// The centres of two lines of text the sidebar draws, one above the other, where a hover changes the frame.
fn sidebar_rows(commands: &[DrawCommand]) -> [(f64, f64); 2] {
    let mut rows = Vec::new();
    for_each_with_matrix(commands, |command, matrix| {
        if let DrawCommand::Text { rect, .. } = command {
            let rect = transform_clip_rect(matrix, *rect);
            if rect.x < 300.0 && rect.y > 120.0 && rect.y < WINDOW_H as f32 - 60.0 {
                rows.push((
                    f64::from(rect.x + rect.width / 2.0),
                    f64::from(rect.y + rect.height / 2.0),
                ));
            }
        }
    });
    let first = *rows.first().expect("the sidebar draws text");
    let second = *rows
        .iter()
        .find(|row| row.1 > first.1 + 16.0)
        .expect("the sidebar draws a second line");
    [first, second]
}

fn bench_workshop(c: &mut Criterion) {
    telar::install_default_text_metrics();
    catalogue::theme::register_modes();
    telar::set_theme(catalogue::theme::AppTheme::light());
    telar::set_surface_size(Size::new(WINDOW_W as f32, WINDOW_H as f32));
    let previews = [
        catalogue::telar_all_previews(),
        telar_components::telar_all_previews(),
        telar_navigate::telar_all_previews(),
    ]
    .concat();
    let app = telar_workshop::WorkshopApp::new(previews);
    let mut tree = ComponentList::new(app.root());
    tree.on_event(&Event::WindowResized {
        width: WINDOW_W,
        height: WINDOW_H,
    });
    let rows = sidebar_rows(&tree.commands());
    let mut frame_at = |row| {
        tree.on_event(&hover(row));
        tree.commands().clone()
    };
    let frames = [frame_at(rows[0]), frame_at(rows[1])];
    assert_ne!(frames[0], frames[1], "the hover changes the frame");

    let mut encoder = FrameEncoder::new();
    let cold = encoder.encode(&frames[0]).len();
    let steady = encoder.encode(&frames[1]).len();
    eprintln!(
        "\n--- workshop_{WINDOW_W}x{WINDOW_H}: {} commands, {cold} bytes cold, {steady} bytes steady ---\n",
        frames[0].len()
    );

    let mut group = c.benchmark_group(format!("boundary/workshop_{WINDOW_W}x{WINDOW_H}"));
    group.bench_function("build", |b| {
        let mut toggle = false;
        b.iter(|| {
            toggle = !toggle;
            tree.on_event(&hover(rows[usize::from(toggle)]));
            black_box(tree.commands().len());
        });
    });
    group.bench_function("clone", |b| b.iter(|| black_box(frames[0].clone())));
    for (label, cold) in [("steady", false), ("cold", true)] {
        let mut codec = Codec::default();
        group.bench_function(format!("encode_{label}"), |b| {
            b.iter_custom(|iters| codec.run(iters, &frames, cold)[0])
        });
        let mut codec = Codec::default();
        group.bench_function(format!("decode_{label}"), |b| {
            b.iter_custom(|iters| codec.run(iters, &frames, cold)[1])
        });
    }
    group.finish();
}

criterion_group!(benches, bench_workshop);
criterion_main!(benches);
