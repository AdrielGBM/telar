/// Byte budget for the decoded image pixmap cache. A single 4K RGBA image is ~33 MiB, so an entry-count cap would let the cache grow into the gigabytes; a byte budget bounds memory regardless of image size.
pub const IMAGE_CACHE_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// Byte budget for the precomputed shadow pixmap cache. Large widget shadows at 1080p can exceed 8 MiB, so an entry-count cap is not a meaningful memory bound.
pub const SHADOW_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Byte budget for the colored-text raster cache (width × height × 4 bytes per entry).
pub const TEXT_PIXEL_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Byte budget for the alpha-only raster cache used for text shadows.
pub const TEXT_ALPHA_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

/// Byte budget for the glyph position-list shaping cache. Each entry is Arc<Vec<(CacheKey, i32, i32)>>; position lists (24 bytes each) scale with text length. A 500-word paragraph = ~12 KB; 2048 entries ≈ 24 MiB uncapped, so byte-weighted LRU bounds memory predictably.
pub const TEXT_SHAPING_CACHE_BUDGET_BYTES: usize = 24 * 1024 * 1024;

/// Maximum number of tiny_skia::Pixmaps cached for text rasterizations.
/// Mirrors the keys of the text pixel cache; sized to cover the in-flight text elements rather than the full texture cache (which is byte-budgeted).
pub const TEXT_PIXMAP_CACHE_MAX_ENTRIES: usize = 256;

/// Byte budget for pre-blurred text shadow pixmaps. Avoids re-running the Gaussian blur every frame for text with shadows. Shadow size is proportional to text bounding box; budget matches the shadow rect cache.
pub const TEXT_SHADOW_CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;

/// Byte budget for pre-blurred path shadow pixmaps. Mirrors the text shadow cache budget.
pub const PATH_SHADOW_CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
