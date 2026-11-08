use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::BenchmarkGroup;
use criterion::measurement::WallTime;
use renderer_core::DrawCommand;
use renderer_core::wire::{FrameDecoder, FrameEncoder};

#[derive(Default)]
pub struct Codec {
    encoder: FrameEncoder,
    decoder: FrameDecoder,
    frame: Vec<u8>,
    out: Vec<DrawCommand>,
}

impl Codec {
    pub fn run(&mut self, iters: u64, frames: &[Vec<DrawCommand>; 2], cold: bool) -> [Duration; 2] {
        let mut spent = [Duration::ZERO; 2];
        for iter in 0..iters {
            if cold {
                self.encoder.reset();
            }
            let start = Instant::now();
            let encoded = black_box(self.encoder.encode(&frames[iter as usize % 2]));
            spent[0] += start.elapsed();
            // Copied outside the timings: the decoder reads the frame while it asks the encoder for pixels, as a host does across the boundary.
            self.frame.clear();
            self.frame.extend_from_slice(encoded);
            let start = Instant::now();
            self.decoder
                .decode(&self.frame, &mut self.out, |id| {
                    self.encoder.pixels(id).map(<[u8]>::to_vec)
                })
                .expect("the frame decodes");
            spent[1] += start.elapsed();
            black_box(&self.out);
        }
        spent
    }
}

pub fn report(name: &str, frames: &[Vec<DrawCommand>; 2]) {
    let mut encoder = FrameEncoder::new();
    let cold = encoder.encode(&frames[0]).len();
    let steady = encoder.encode(&frames[1]).len();
    eprintln!(
        "\n--- {name}: {} commands, {cold} bytes cold, {steady} bytes steady ---\n",
        frames[0].len()
    );
}

pub fn bench_codec(group: &mut BenchmarkGroup<'_, WallTime>, frames: &[Vec<DrawCommand>; 2]) {
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
}
