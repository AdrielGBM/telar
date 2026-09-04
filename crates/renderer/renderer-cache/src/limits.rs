//! Every cache bound in the renderer, in one place.
//!
//! They live together because the thing that went wrong with them could only be seen by comparing them. Spread over four crates, each number looked defensible on its own; side by side, a shell's text rasters were budgeted at 152 MB against Skia's 2 MB glyph cache, the GPU texture cache had no size bound at all, and the two backends evicted the same content by opposite rules. A number that cannot be compared cannot be justified.
//!
//! Horizons are wall-clock for every cache, including the GPU ones that used to count frames. Frame counts made the GPU bounds depend on refresh rate — 60 frames is one second at 60 Hz and 0.4 at 144 — and made them incomparable with the CPU side. The durations below are the 60 Hz equivalents of the frame counts they replace.

use std::time::Duration;

use crate::Policy;

/// Bytes of cache per pixel of surface, from Flutter's `width × height × 12 × 4` resource budget.
///
/// A budget that is the same number on a 4K desktop and on a phone is wrong on one of them. Flutter derives its from the display for this reason; these caches derive theirs from the surface they draw into, which on a desktop shell — a bar a few hundred pixels tall — is far smaller than the display.
const SURFACE_BYTES_PER_PIXEL: usize = 48;
/// Enough to hold a 4K image — ~33 MiB — on a surface whose own size would derive far less. A bar a few hundred pixels tall can still be asked to draw a wallpaper thumbnail, and a budget under one image's size means it is rebuilt from its source bytes on every frame forever.
///
/// Generous floors cost nothing on their own: a budget is a ceiling on what the cache *may* hold, not memory it reserves. What made the old flat 256 MB expensive was not the number but that nothing ever gave the bytes back — an LRU only evicts under pressure, so the high-water mark stood for the life of the process. The idle sweep is what makes the ceiling safe to be roomy.
const SURFACE_BUDGET_FLOOR_BYTES: usize = 64 * 1024 * 1024;
const SURFACE_BUDGET_CEIL_BYTES: usize = 128 * 1024 * 1024;

/// The budget for a cache whose contents scale with the surface: decoded images, GPU textures.
pub fn surface_budget_bytes(surface_width: u32, surface_height: u32) -> usize {
    (surface_width as usize)
        .saturating_mul(surface_height as usize)
        .saturating_mul(SURFACE_BYTES_PER_PIXEL)
        .clamp(SURFACE_BUDGET_FLOOR_BYTES, SURFACE_BUDGET_CEIL_BYTES)
}

/// How long a CPU-side raster survives unasked-for.
///
/// Far longer than the toolkits that do this at all — GTK4's GPU cache collects on a 15-second timeout by default, and Qt's `QPixmapCache` trims about 25% per minute once the app goes idle — because "unasked-for" here means *not redrawn*, not *not visible*. Dirty-region culling skips the draw for anything outside the changed rectangle, so a label sitting in plain sight on a static bar goes untouched while the clock beside it ticks. A horizon in seconds would evict what is on screen; ten minutes bounds the damage to content that really has gone away — a closed menu, a hidden surface — and costs one reshape if it guesses wrong.
///
/// Text and shadows share it. They were split at first on the theory that a blur recomputes faster than a paragraph reshapes, which the code disproves: shadows above `ASYNC_SHADOW_THRESHOLD` are handed to a background thread precisely because the blur is too slow to do inline.
pub const CPU_IDLE: Duration = Duration::from_secs(600);
/// GPU resources, at the 60 Hz equivalent of the frame counts these replace, and in the range GTK4 uses for the same job. VRAM is scarcer than RAM and a re-upload is cheaper than a reshape, so the GPU side can afford to let go on a horizon the CPU side could not.
pub const GPU_TEXTURE_IDLE: Duration = Duration::from_secs(1);
/// How long tessellated geometry may sit unused before it is dropped.
pub const GPU_GEOMETRY_IDLE: Duration = Duration::from_secs(2);

/// Composed strings rasterized to premultiplied RGBA, at the size and colour they were drawn.
///
/// No reference renderer caches a composed line as pixels — Skia, Android, Flutter and WebRender cache glyphs and composite the line from an atlas, which on a GPU is a handful of nearly-free blits. The CPU backend composites glyph by glyph with alpha blending, so keeping the finished line does buy something the GPU backends would not need. What it must not do is keep text that repeats zero times, which is what admission is for.
pub const TEXT_RASTER: Policy = Policy::new(16 * 1024 * 1024)
    .idle(CPU_IDLE)
    .admit_on_second_use();

/// Glyph position lists, `(CacheKey, i32, i32)` per glyph.
///
/// No admission, unlike the raster cache: entries are a couple of hundred bytes, so the budget bounds them without help, and the cost of admission here would be reshaping a string — the expensive half of drawing text — to save bytes the budget already caps.
pub const TEXT_SHAPING: Policy = Policy::new(4 * 1024 * 1024).idle(CPU_IDLE);

/// Measured `(width, height)` per string, size and style, and whether a string shapes to any COLR glyph.
///
/// Both used to cap an entry count, being far too small for their size to be the thing worth bounding — two `f32` and a `bool`. They are in bytes here anyway, at [`SMALL_ENTRY_BYTES`] apiece, so that every bound in this file reads in one unit. 64 KB is roughly two thousand strings each, well past the count they capped at.
pub const TEXT_MEASURE: Policy = Policy::new(64 * 1024).idle(CPU_IDLE);
/// The has-COLR flags: tiny entries, so they are bounded by size alone.
pub const TEXT_HAS_COLR: Policy = Policy::new(64 * 1024).idle(CPU_IDLE);

/// What one entry of a small keyed lookup is charged: its key, its value, and the map's own per-entry overhead, rounded to something legible rather than measured exactly. Precision here would buy nothing — the budget exists to stop unbounded growth, not to account for kilobytes.
pub const SMALL_ENTRY_BYTES: usize = 32;

/// Ceiling on the rasterized glyph masks cosmic-text's `SwashCache` holds.
///
/// Not a [`Policy`], because `SwashCache` is a pair of plain `HashMap`s owned by cosmic-text with no eviction API and no per-entry recency — so there is no basis on which to choose *which* glyph to drop, and the only honest policy is to drop all of them once the whole thing is too big. Deliberately far above the working set of a desktop shell (a few hundred glyphs across a handful of sizes and subpixel bins, on the order of a megabyte), so that a normal session never clears and only one that has accumulated many fonts or sizes ever pays the re-warm.
pub const GLYPH_RASTER_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// Raw font files, kept so COLR rasterization does not re-read one on every atlas miss.
///
/// Whole files, and previously unbounded: a colour emoji font is tens of megabytes, so "bounded in practice by the installed fonts" was a bound measured in the wrong unit.
pub const FONT_FILE: Policy = Policy::new(32 * 1024 * 1024).idle(CPU_IDLE);

/// Pre-blurred rectangle shadows, keyed by geometry, radii and colour.
pub const SHADOW: Policy = Policy::new(24 * 1024 * 1024).idle(CPU_IDLE);
/// Pre-blurred text shadows.
///
/// Takes no admission, though it is the one shadow cache keyed by a *string* and so the one whose key space is unbounded — on a live shell it was 91% of all cache growth over twenty minutes, which is exactly the shape admission exists for.
///
/// It would cost more than it saves here. Shadows past `ASYNC_SHADOW_THRESHOLD` are blurred on a background thread, and a rejected first sighting discards that finished work: the entry leaves `pending`, nothing draws, and the next frame spawns the whole blur again. Worse, the threshold sits at 80 000 px and these entries run about 87 000, so admission would bite precisely the expensive ones while sparing the small ones — saving the megabytes that are not there and paying double for the ones that are. The byte budget and the idle horizon bound it without that bargain.
pub const TEXT_SHADOW: Policy = Policy::new(8 * 1024 * 1024).idle(CPU_IDLE);
/// Pre-blurred path shadows.
pub const PATH_SHADOW: Policy = Policy::new(8 * 1024 * 1024).idle(CPU_IDLE);

/// Uploaded GPU image textures, keyed by content hash and filter. Surface-derived, and bounded by size at all — the hand-rolled cache this replaces bounded only by age, so a handful of 4K images was ~33 MB of VRAM each with no ceiling on how many.
pub fn gpu_texture(surface_width: u32, surface_height: u32) -> Policy {
    Policy::new(surface_budget_bytes(surface_width, surface_height)).idle(GPU_TEXTURE_IDLE)
}

/// Tessellated path geometry: vertices and indices. Also previously age-bounded only.
pub const GPU_PATH_TESS: Policy = Policy::new(8 * 1024 * 1024).idle(GPU_GEOMETRY_IDLE);

/// Resolved shadow textures on the GPU.
pub const GPU_SHADOW: Policy = Policy::new(32 * 1024 * 1024).idle(GPU_GEOMETRY_IDLE);
