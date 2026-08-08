/// Viewport buffer/bind-group slots pre-allocated for per-layer uniforms; frames with more concurrent layers fall back to ad-hoc allocations.
pub const VIEWPORT_POOL_SIZE: usize = 8;

/// Upper bound on cached scratch textures per (width, height, format); prevents unbounded GPU memory growth.
pub const MAX_TEXTURE_POOL_PER_SIZE: usize = 4;
