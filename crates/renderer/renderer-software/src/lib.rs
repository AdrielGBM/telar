mod budget;
mod caches;
mod primitives;
pub(crate) mod renderer;

pub use budget::SoftwareRendererConfig;
pub use caches::sweep_idle;
pub use renderer::SoftwareRenderer;
pub use renderer_cache::CacheStat;
pub use renderer_cache::registry::snapshot as cache_stats;
