//! The knobs a caller overrides to size the software backend for its own workload.

/// What the software backend's caches may hold, plus font config. Pass to `SoftwareRenderer::new()` to override the defaults in [`renderer_cache::limits`].
///
/// A whole [`Policy`](renderer_cache::Policy) per cache rather than a byte count, so an app can also say how long an entry may sit idle and whether one sighting is enough to keep it. A shell and a photo viewer want different answers to all three.
pub struct SoftwareRendererConfig {
    pub font: renderer_core::FontConfig,
    /// The app wants a transparent surface. On Wayland this switches presentation from softbuffer (opaque XRGB) to an own `wl_shm` ARGB8888 buffer that preserves alpha; elsewhere it is currently a no-op (softbuffer stays opaque).
    pub transparent: bool,
    /// The window system keeps what was last presented on screen, so a present declares only the regions that changed. Leave it `false` unless the platform guarantees that: on a surface that does not keep its contents, a region another window uncovers would never be repainted.
    pub retains_presented_contents: bool,
}

impl Default for SoftwareRendererConfig {
    fn default() -> Self {
        Self {
            font: renderer_core::FontConfig::default(),
            transparent: false,
            retains_presented_contents: false,
        }
    }
}
