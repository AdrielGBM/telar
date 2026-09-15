//! Gated on the `TELAR_PERF` env var or a scoped [`capture`]; when neither is on, every entry point costs only a cached flag read plus one relaxed-atomic load, so it is safe to leave the instrumentation compiled into release builds.

use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use web_time::Instant;

/// Frame phases attributed to CPU vs GPU so a baseline can split the ~16 ms budget.
#[derive(Clone, Copy)]
pub enum Phase {
    /// UI thread: `tree.commands()` flatten + `dev.on_frame`.
    Build = 0,
    /// UI thread: the per-frame `Vec<DrawCommand>` clone handed to the render thread.
    Clone = 1,
    /// Hardware: `analyze_frame` (dirty/scroll detection) + `interpret_commands`. Software: clearing and rasterizing the commands into the pixmap.
    Interpret = 2,
    /// Hardware: segment build + pass execution + `queue.submit` (encompasses `present`).
    Gpu = 3,
    /// A renderer's whole `render_frame`, on whichever thread drives it.
    Frame = 4,
    /// Hardware: `output.present()` alone — a subset of `gpu` that isolates swapchain/vsync block (FIFO present on mobile) from the CPU-side command-buffer build + submit. Software: acquiring the surface buffer, filling it and committing it, a superset of `convert`.
    Present = 5,
    /// Software: dirty planning and the scroll blit, ahead of `interpret`.
    Plan = 6,
    /// Software: converting the pixmap into the surface buffer, a subset of `present`.
    Convert = 7,
}

const N: usize = 8;
const NAMES: [&str; N] = [
    "build",
    "clone",
    "interpret",
    "gpu",
    "frame",
    "present",
    "plan",
    "convert",
];
// Dump cadence in ticked frames; one line per ~second at 60 fps keeps logcat readable.
const DUMP_EVERY: u64 = 60;

struct Stats {
    sums: [AtomicU64; N],
    counts: [AtomicU64; N],
    maxes: [AtomicU64; N],
    // Frames that took the damage-tracking path, so the dump shows whether damage is actually firing (vs falling back to a full repaint).
    damage_frames: AtomicU64,
    damaged_px: AtomicU64,
    surface_px: AtomicU64,
}

impl Stats {
    const fn new() -> Self {
        Self {
            sums: [const { AtomicU64::new(0) }; N],
            counts: [const { AtomicU64::new(0) }; N],
            maxes: [const { AtomicU64::new(0) }; N],
            damage_frames: AtomicU64::new(0),
            damaged_px: AtomicU64::new(0),
            surface_px: AtomicU64::new(0),
        }
    }

    fn record(&self, phase: Phase, dur: Duration) {
        let i = phase as usize;
        let ns = dur.as_nanos() as u64;
        self.sums[i].fetch_add(ns, Ordering::Relaxed);
        self.counts[i].fetch_add(1, Ordering::Relaxed);
        self.maxes[i].fetch_max(ns, Ordering::Relaxed);
    }

    fn take(&self) -> Snapshot {
        let take =
            |counters: &[AtomicU64; N]| counters.each_ref().map(|c| c.swap(0, Ordering::Relaxed));
        Snapshot {
            sums: take(&self.sums),
            counts: take(&self.counts),
            maxes: take(&self.maxes),
            damage_frames: self.damage_frames.swap(0, Ordering::Relaxed),
            damaged_px: self.damaged_px.swap(0, Ordering::Relaxed),
            surface_px: self.surface_px.swap(0, Ordering::Relaxed),
        }
    }
}

static ENABLED: OnceLock<bool> = OnceLock::new();
static GLOBAL: Stats = Stats::new();
static FRAMES: AtomicU64 = AtomicU64::new(0);
static CAPTURES: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    static CAPTURED: Stats = const { Stats::new() };
}

fn env_enabled() -> bool {
    *ENABLED.get_or_init(
        || matches!(std::env::var("TELAR_PERF").as_deref(), Ok(v) if !v.is_empty() && v != "0"),
    )
}

fn capturing_here() -> bool {
    CAPTURES.load(Ordering::Relaxed) > 0 && CAPTURING.with(Cell::get)
}

#[inline]
fn enabled() -> bool {
    env_enabled() || CAPTURES.load(Ordering::Relaxed) > 0
}

// A capturing thread records only into its capture, and every other thread only into the env-gated global, so a capture neither sees nor leaks another thread's frames.
fn with_sink(f: impl FnOnce(&Stats)) {
    if capturing_here() {
        CAPTURED.with(f);
    } else if env_enabled() {
        f(&GLOBAL);
    }
}

#[inline]
pub fn note_damage(active: bool) {
    if active && enabled() {
        with_sink(|s| {
            s.damage_frames.fetch_add(1, Ordering::Relaxed);
        });
    }
}

#[inline]
pub fn note_damage_area(damaged_px: u64, surface_px: u64) {
    if enabled() {
        with_sink(|s| {
            s.damaged_px.fetch_add(damaged_px, Ordering::Relaxed);
            s.surface_px.fetch_add(surface_px, Ordering::Relaxed);
        });
    }
}

/// `Instant::now()` only when instrumentation is on, so disabled builds never read the clock.
#[inline]
pub fn now_if_enabled() -> Option<Instant> {
    if enabled() {
        Some(Instant::now())
    } else {
        None
    }
}

/// Record the elapsed time since a `now_if_enabled()` mark; a no-op when disabled.
#[inline]
pub fn record_since(phase: Phase, start: Option<Instant>) {
    if let Some(t) = start {
        let dur = t.elapsed();
        with_sink(|s| s.record(phase, dur));
    }
}

/// RAII span that records into `phase` on drop. `None` when disabled.
pub struct Span {
    phase: Phase,
    start: Instant,
}

#[inline]
/// Starts timing a phase, or returns `None` when profiling is off.
pub fn span(phase: Phase) -> Option<Span> {
    if enabled() {
        Some(Span {
            phase,
            start: Instant::now(),
        })
    } else {
        None
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let dur = self.start.elapsed();
        with_sink(|s| s.record(self.phase, dur));
    }
}

/// What was recorded over a window of frames: the rolling dump's, or a [`capture`]'s.
pub struct Snapshot {
    sums: [u64; N],
    counts: [u64; N],
    maxes: [u64; N],
    damage_frames: u64,
    damaged_px: u64,
    surface_px: u64,
}

impl Snapshot {
    pub fn count(&self, phase: Phase) -> u64 {
        self.counts[phase as usize]
    }

    pub fn damage_frames(&self) -> u64 {
        self.damage_frames
    }

    /// Damaged pixels over surface pixels, summed across every noted frame; `None` when none was noted.
    pub fn damaged_fraction(&self) -> Option<f64> {
        (self.surface_px > 0).then(|| self.damaged_px as f64 / self.surface_px as f64)
    }

    fn summary(&self) -> String {
        let mut parts = String::new();
        for (i, name) in NAMES.iter().enumerate() {
            let cnt = self.counts[i];
            if cnt == 0 {
                continue;
            }
            let avg_us = (self.sums[i] as f64 / cnt as f64) / 1000.0;
            let max_us = self.maxes[i] as f64 / 1000.0;
            parts.push_str(&format!("{name}={avg_us:.0}/{max_us:.0}us(n{cnt}) "));
        }
        parts.push_str(&format!("damage={}", self.damage_frames));
        if let Some(fraction) = self.damaged_fraction() {
            parts.push_str(&format!(" area={:.1}%", fraction * 100.0));
        }
        parts
    }
}

/// Runs `f` with instrumentation on for the calling thread only, returning what it recorded. Nothing recorded on other threads, or into the `TELAR_PERF` dump, reaches the snapshot, and nothing `f` records reaches the dump. Does not nest.
pub fn capture<R>(f: impl FnOnce() -> R) -> (R, Snapshot) {
    struct Capturing;
    impl Drop for Capturing {
        fn drop(&mut self) {
            CAPTURING.with(|c| c.set(false));
            CAPTURES.fetch_sub(1, Ordering::Relaxed);
        }
    }

    assert!(!CAPTURING.with(Cell::get), "perf::capture does not nest");
    CAPTURED.with(Stats::take);
    CAPTURING.with(|c| c.set(true));
    CAPTURES.fetch_add(1, Ordering::Relaxed);
    let guard = Capturing;
    let result = f();
    drop(guard);
    (result, CAPTURED.with(Stats::take))
}

/// Advance the frame counter and, every `DUMP_EVERY` frames, log rolling avg/max per phase and reset the accumulators. Call once per frame from the thread that owns the frame loop.
pub fn tick() {
    if capturing_here() || !env_enabled() {
        return;
    }
    let f = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if !f.is_multiple_of(DUMP_EVERY) {
        return;
    }
    tracing::info!(target: "telar_perf", "perf[{DUMP_EVERY}f] {}", GLOBAL.take().summary());
}

#[cfg(test)]
#[path = "perf_test.rs"]
mod tests;
