//! Lightweight per-phase frame timing, gated on the `TELAR_PERF` env var. When disabled every entry point is a single relaxed-atomic load or an early return, so it is safe to leave the instrumentation compiled into release builds. Enabled it accumulates per-phase durations across both the UI thread (command build/clone) and the render thread (interpret/gpu) and dumps rolling averages via `tracing` — stdout on desktop, logcat on Android.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use web_time::Instant;

/// Frame phases attributed to CPU vs GPU so a baseline can split the ~16 ms budget.
#[derive(Clone, Copy)]
pub enum Phase {
    /// UI thread: `tree.commands()` flatten + `dev.on_frame`.
    Build = 0,
    /// UI thread: the per-frame `Vec<DrawCommand>` clone handed to the render thread.
    Clone = 1,
    /// Render thread: `analyze_frame` (dirty/scroll detection) + `interpret_commands`.
    Interpret = 2,
    /// Render thread: segment build + pass execution + `queue.submit` (encompasses `present`).
    Gpu = 3,
    /// Whole render_frame (render thread) or whole SW render (UI thread).
    Frame = 4,
    /// Render thread: `output.present()` alone — a subset of `gpu` that isolates swapchain/vsync block (FIFO present on mobile) from the CPU-side command-buffer build + submit.
    Present = 5,
}

const N: usize = 6;
const NAMES: [&str; N] = ["build", "clone", "interpret", "gpu", "frame", "present"];
// Dump cadence in ticked frames; one line per ~second at 60 fps keeps logcat readable.
const DUMP_EVERY: u64 = 60;

static ENABLED: OnceLock<bool> = OnceLock::new();
static SUMS: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static COUNTS: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static MAXES: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static FRAMES: AtomicU64 = AtomicU64::new(0);
// Count of frames in the current window that took the F1 damage-tracking path, so the dump shows whether damage is actually firing (vs falling back to a full repaint).
static DAMAGE_FRAMES: AtomicU64 = AtomicU64::new(0);

/// Record whether the frame being rendered used F1 damage tracking.
#[inline]
pub fn note_damage(active: bool) {
    if active && enabled() {
        DAMAGE_FRAMES.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
fn enabled() -> bool {
    *ENABLED.get_or_init(
        || matches!(std::env::var("TELAR_PERF").as_deref(), Ok(v) if !v.is_empty() && v != "0"),
    )
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

#[inline]
fn record(phase: Phase, dur: Duration) {
    let i = phase as usize;
    let ns = dur.as_nanos() as u64;
    SUMS[i].fetch_add(ns, Ordering::Relaxed);
    COUNTS[i].fetch_add(1, Ordering::Relaxed);
    MAXES[i].fetch_max(ns, Ordering::Relaxed);
}

/// Record the elapsed time since a `now_if_enabled()` mark; a no-op when disabled.
#[inline]
pub fn record_since(phase: Phase, start: Option<Instant>) {
    if let Some(t) = start {
        record(phase, t.elapsed());
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
        record(self.phase, self.start.elapsed());
    }
}

/// Advance the frame counter and, every `DUMP_EVERY` frames, log rolling avg/max per phase and reset the accumulators. Call once per frame from the thread that owns the frame loop.
pub fn tick() {
    if !enabled() {
        return;
    }
    let f = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if f % DUMP_EVERY != 0 {
        return;
    }
    let mut parts = String::new();
    for i in 0..N {
        let sum = SUMS[i].swap(0, Ordering::Relaxed);
        let cnt = COUNTS[i].swap(0, Ordering::Relaxed);
        let mx = MAXES[i].swap(0, Ordering::Relaxed);
        if cnt == 0 {
            continue;
        }
        let avg_us = (sum as f64 / cnt as f64) / 1000.0;
        let max_us = mx as f64 / 1000.0;
        parts.push_str(&format!("{}={avg_us:.0}/{max_us:.0}us(n{cnt}) ", NAMES[i]));
    }
    let damage = DAMAGE_FRAMES.swap(0, Ordering::Relaxed);
    tracing::info!(target: "telar_perf", "perf[{DUMP_EVERY}f] {}damage={damage}", parts);
}
