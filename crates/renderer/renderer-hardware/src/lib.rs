//! The wgpu backend: instanced pipelines per primitive, layers rendered into pooled textures, and a per-frame damage path that repaints only what changed.

#![warn(rustdoc::broken_intra_doc_links)]

mod blur;
mod caches;
mod composite;
mod config;
pub mod gpu;
pub mod limits;
mod pass;
mod primitives;
pub(crate) mod renderer;

pub use config::HardwareRendererConfig;
pub use renderer::HardwareRenderer;
