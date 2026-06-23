// Cache age and pool-size budgets for the hardware renderer.
// Edit this file to tune all GPU cache ages and pool sizes from one place.

/// Frames an unused GPU image texture survives before eviction. ~1 s at 60 fps; GPU textures are expensive so evict aggressively.
pub const IMAGE_GPU_MAX_AGE_FRAMES: u64 = 60;

/// Frames a cached path tessellation survives without use before eviction. ~2 s at 60 fps.
pub const PATH_TESS_MAX_AGE_FRAMES: u64 = 120;

/// Viewport buffer/bind-group slots pre-allocated for per-layer uniforms; frames with more concurrent layers fall back to ad-hoc allocations.
pub const VIEWPORT_POOL_SIZE: usize = 8;

/// Upper bound on cached scratch textures per (width, height, format); prevents unbounded GPU memory growth.
pub const MAX_TEXTURE_POOL_PER_SIZE: usize = 4;
