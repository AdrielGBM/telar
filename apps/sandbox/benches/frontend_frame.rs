//! What a sandbox frame costs on the terminal against what it costs on the desktop.
//!
//! The frontends share everything above the backend seam: the same components, the same layout pass, the same `DrawCommand` stream. What differs is who consumes that stream — a rasteriser filling a million-pixel `Pixmap`, or a painter filling a few thousand character cells — and what a string measures to, since the terminal installs `CellMetrics` in place of the shaper.
//!
//! So the comparison is made three ways, because no one of them answers the question honestly on its own:
//!
//! * `paint` feeds *the same* command list to all three backends over *the same* logical surface. Nothing but the backend varies, so the ratio is the backend's.
//! * `paint_native` runs each at the size it really works at — a 1200×900 window against an 80×24 and a 120×40 terminal — which is the difference a user actually feels, size included.
//! * `frame` drives a hover through the real tree and repaints, so the backend's share of a whole frame is visible rather than assumed.

use std::hint::black_box;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{Criterion, criterion_group, criterion_main};
use platform_core::PointerSource;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, RenderBackend};
use renderer_hardware::HardwareRenderer;
use renderer_software::{SoftwareRenderer, SoftwareRendererConfig};
use renderer_tui::{CellMetrics, CellSize, ColorDepth, TuiConfig, TuiRenderer};
use telar::{App, Event};

const WINDOW_W: u32 = 1200;
const WINDOW_H: u32 = 900;

const CLEAR: Color = Color {
    r: 0.06,
    g: 0.06,
    b: 0.09,
    a: 1.0,
};
/// A second clear colour, alternated with [`CLEAR`], so neither backend's "this frame equals the last" fast path turns the benchmark into a no-op.
const CLEAR_ALT: Color = Color {
    r: 0.062,
    g: 0.06,
    b: 0.09,
    a: 1.0,
};

/// The terminal writes escape sequences rather than pixels, and how many is the other half of what a terminal frame costs. A real stdout cannot be benchmarked, so the bytes are counted and thrown away, and reported alongside the timings.
#[derive(Clone, Default)]
struct CountingSink(Arc<AtomicUsize>);

impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.fetch_add(buf.len(), Ordering::Relaxed);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl CountingSink {
    fn take(&self) -> usize {
        self.0.swap(0, Ordering::Relaxed)
    }
}

fn use_shaper_metrics() {
    renderer_core::set_text_metrics(renderer_text::ShaperMetrics);
}

fn use_cell_metrics(cell: CellSize) {
    renderer_core::set_text_metrics(CellMetrics::new(cell));
}

/// The real sandbox tree, laid out at `width`×`height` logical pixels, flattened to the command stream a backend receives. Whichever measurer is installed when this runs is the one the layout is sized by.
fn tree_at(width: u32, height: u32) -> telar::ComponentList {
    telar::set_theme(sandbox::core::theme::SandboxTheme::modern());
    let mut tree = telar::ComponentList::new(sandbox::core::app::SandboxRoot.root());
    tree.on_event(&Event::WindowResized { width, height });
    tree
}

fn commands_at(width: u32, height: u32) -> Vec<DrawCommand> {
    tree_at(width, height).commands().to_vec()
}

fn software(width: u32, height: u32) -> SoftwareRenderer<HeadlessWindow, HeadlessWindow> {
    SoftwareRenderer::new_headless(width, height, SoftwareRendererConfig::default())
}

/// The backend a desktop window actually gets when the machine has a GPU, which is why it is here at all: the CPU renderer below is the fallback, and comparing the terminal only against the fallback would flatter it. `None` where no adapter can be opened, so the run reports what it measured instead of inventing a number.
fn hardware(width: u32, height: u32) -> Option<HardwareRenderer<HeadlessWindow>> {
    match pollster::block_on(HardwareRenderer::<HeadlessWindow>::new_headless(
        width,
        height,
        None,
        false,
        renderer_text::TextShaperConfig::default(),
    )) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("no GPU adapter, skipping the hardware backend: {e:?}");
            None
        }
    }
}

fn tui(sink: CountingSink) -> TuiRenderer {
    TuiRenderer::new(
        TuiConfig {
            cell: CellSize::default(),
            // Pinned rather than detected: a benchmark run under a pipe would otherwise quantise to a different palette than a benchmark run in a terminal, and truecolor is both the common case and the most expensive one to encode.
            depth: ColorDepth::TrueColor,
            ..TuiConfig::default()
        },
        Box::new(sink),
    )
}

/// One frame through a backend, with the clear colour alternated so no unchanged-frame shortcut is taken.
fn paint(backend: &mut dyn RenderBackend, cmds: &[DrawCommand], w: u32, h: u32, toggle: &mut bool) {
    let clear = if *toggle { CLEAR } else { CLEAR_ALT };
    *toggle = !*toggle;
    backend.begin_frame(w, h, 1.0, 0).unwrap();
    backend.render_frame(black_box(cmds), Some(clear)).unwrap();
}

/// The same command stream, the same logical surface, three backends. The only variable is who draws.
fn bench_paint(c: &mut Criterion) {
    use_shaper_metrics();
    let cmds = commands_at(WINDOW_W, WINDOW_H);
    report(&cmds);

    let mut group = c.benchmark_group("paint/1200x900");

    if let Some(mut gpu) = hardware(WINDOW_W, WINDOW_H) {
        group.bench_function("hardware", |b| {
            let mut toggle = false;
            b.iter(|| {
                paint(&mut gpu, &cmds, WINDOW_W, WINDOW_H, &mut toggle);
                // Submission is asynchronous, so without this the timing would be of handing work to the driver rather than of doing it.
                gpu.wait_idle().unwrap();
            });
        });
    }

    let mut sw = software(WINDOW_W, WINDOW_H);
    group.bench_function("software", |b| {
        let mut toggle = false;
        b.iter(|| paint(&mut sw, &cmds, WINDOW_W, WINDOW_H, &mut toggle));
    });

    let sink = CountingSink::default();
    let mut term = tui(sink.clone());
    group.bench_function("tui", |b| {
        let mut toggle = false;
        b.iter(|| paint(&mut term, &cmds, WINDOW_W, WINDOW_H, &mut toggle));
    });
    group.finish();
}

/// Each frontend at the surface it really runs on: a window against two ordinary terminals. Each tree is laid out under its own measurer, so these are the frames the two frontends genuinely draw.
fn bench_paint_native(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_native");

    use_shaper_metrics();
    let desktop = commands_at(WINDOW_W, WINDOW_H);

    if let Some(mut gpu) = hardware(WINDOW_W, WINDOW_H) {
        group.bench_function("hardware/1200x900", |b| {
            let mut toggle = false;
            b.iter(|| {
                paint(&mut gpu, &desktop, WINDOW_W, WINDOW_H, &mut toggle);
                gpu.wait_idle().unwrap();
            });
        });
    }

    let mut sw = software(WINDOW_W, WINDOW_H);
    group.bench_function("software/1200x900", |b| {
        let mut toggle = false;
        b.iter(|| paint(&mut sw, &desktop, WINDOW_W, WINDOW_H, &mut toggle));
    });

    let cell = CellSize::default();
    use_cell_metrics(cell);
    for (cols, rows) in [(80u32, 24u32), (120, 40)] {
        let (w, h) = (
            (cols as f32 * cell.width) as u32,
            (rows as f32 * cell.height) as u32,
        );
        let cmds = commands_at(w, h);
        let sink = CountingSink::default();
        let mut term = tui(sink.clone());
        group.bench_function(format!("tui/{cols}x{rows}"), |b| {
            let mut toggle = false;
            b.iter(|| paint(&mut term, &cmds, w, h, &mut toggle));
        });
        eprintln!(
            "  tui/{cols}x{rows}: {} commands, {} cells",
            cmds.len(),
            cols * rows
        );
    }
    group.finish();
}

/// A whole frame: a hover moves, the tree recomposes, the backend draws. The backend's share of that is the number that decides whether "lighter renderer" means "lighter frame".
fn bench_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame/1200x900");
    // Two nav-rail buttons, hovered alternately: a small, real change that dirties the tree every iteration without swapping the whole section.
    let hover = |y: f64| Event::PointerMoved {
        x: 110.0,
        y,
        source: PointerSource::Mouse,
    };

    use_shaper_metrics();
    let mut tree = tree_at(WINDOW_W, WINDOW_H);
    let mut sw = software(WINDOW_W, WINDOW_H);
    group.bench_function("software", |b| {
        let mut toggle = false;
        b.iter(|| {
            tree.on_event(&hover(if toggle { 489.0 } else { 453.0 }));
            let cmds = tree.commands();
            paint(&mut sw, &cmds, WINDOW_W, WINDOW_H, &mut toggle);
        });
    });

    use_cell_metrics(CellSize::default());
    let mut tree = tree_at(WINDOW_W, WINDOW_H);
    let sink = CountingSink::default();
    let mut term = tui(sink.clone());
    group.bench_function("tui", |b| {
        let mut toggle = false;
        b.iter(|| {
            tree.on_event(&hover(if toggle { 489.0 } else { 453.0 }));
            let cmds = tree.commands();
            paint(&mut term, &cmds, WINDOW_W, WINDOW_H, &mut toggle);
        });
    });
    group.finish();
}

/// Descriptive context printed once: what one frame is made of, and what each backend has to move to present it. A still terminal frame writes nothing at all, which no timing below captures on its own.
fn report(cmds: &[DrawCommand]) {
    let sink = CountingSink::default();
    let mut term = tui(sink.clone());

    term.begin_frame(WINDOW_W, WINDOW_H, 1.0, 0).unwrap();
    term.render_frame(cmds, Some(CLEAR)).unwrap();
    let first = sink.take();

    term.begin_frame(WINDOW_W, WINDOW_H, 1.0, 0).unwrap();
    term.render_frame(cmds, Some(CLEAR)).unwrap();
    let still = sink.take();

    term.begin_frame(WINDOW_W, WINDOW_H, 1.0, 0).unwrap();
    term.render_frame(cmds, Some(CLEAR_ALT)).unwrap();
    let changed = sink.take();

    let cell = CellSize::default();
    let (cols, rows) = (
        (WINDOW_W as f32 / cell.width) as usize,
        (WINDOW_H as f32 / cell.height) as usize,
    );
    eprintln!("\n--- one sandbox frame at {WINDOW_W}x{WINDOW_H} logical ---");
    eprintln!("  draw commands:       {}", cmds.len());
    eprintln!(
        "  software surface:    {} px ({} KiB of RGBA)",
        WINDOW_W as usize * WINDOW_H as usize,
        WINDOW_W as usize * WINDOW_H as usize * 4 / 1024
    );
    eprintln!(
        "  tui surface:         {cols}x{rows} = {} cells",
        cols * rows
    );
    eprintln!("  tui bytes, first:    {first}");
    eprintln!("  tui bytes, unchanged:{still}");
    eprintln!("  tui bytes, changed:  {changed}\n");
}

criterion_group!(benches, bench_paint, bench_paint_native, bench_frame);
criterion_main!(benches);
