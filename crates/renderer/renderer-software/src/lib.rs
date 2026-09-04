//! The CPU backend: tiny-skia rasterization into a pixmap, presented through softbuffer.
//!
//! Carries the damage tracking the GPU path also uses, plus a scroll blit and background shadow workers, because a frame it cannot skip is a frame it has to draw a pixel at a time.

mod budget;
mod caches;
mod primitives;
pub(crate) mod renderer;

pub use budget::SoftwareRendererConfig;
pub use caches::sweep_idle;
pub use renderer::SoftwareRenderer;
pub use renderer_cache::CacheStat;
pub use renderer_cache::registry::snapshot as cache_stats;
