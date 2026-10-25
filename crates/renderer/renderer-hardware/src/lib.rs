//! The wgpu backend: instanced pipelines per primitive, layers rendered into pooled textures, and a per-frame damage path that repaints only what changed.

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
compile_error!(
    "telar-renderer-hardware enables wgpu's `fragile-send-sync-non-atomic-wasm`, which is only sound without wasm `atomics`; drop `+atomics` from the wasm target features"
);

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
