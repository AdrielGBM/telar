// Cache budget constants for the software renderer.
// Edit this file to tune all cache memory limits from one place.

/// Maximum number of decoded image pixmaps held in memory.
/// TODO: switch to byte-based budget (see performance.md §8.1).
pub const IMAGE_CACHE_MAX_ENTRIES: usize = 256;

/// Maximum number of precomputed shadow pixmaps held in memory.
/// TODO: switch to byte-based budget (see performance.md §8.1).
pub const SHADOW_CACHE_MAX_ENTRIES: usize = 64;

/// Byte budget for the colored-text raster cache (width × height × 4 bytes per entry).
pub const TEXT_PIXEL_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Byte budget for the alpha-only raster cache used for text shadows.
pub const TEXT_ALPHA_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of shaped-glyph-position lists held in the shaping cache.
/// TODO: switch to byte-based budget (see performance.md §8.4).
pub const TEXT_SHAPING_CACHE_CAPACITY: usize = 2048;

/// Maximum number of tiny_skia::Pixmaps cached for text rasterizations.
/// Mirrors the keys of the text pixel cache; sized to cover the in-flight text elements rather than the full texture cache (which is byte-budgeted).
pub const TEXT_PIXMAP_CACHE_MAX_ENTRIES: usize = 256;
