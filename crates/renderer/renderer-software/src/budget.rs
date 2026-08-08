use renderer_cache::{Policy, limits};

/// What the software backend's caches may hold, plus font config. Pass to `SoftwareRenderer::new()` to override the
/// defaults in [`renderer_cache::limits`].
///
/// A whole [`Policy`] per cache rather than a byte count, so an app can also say how long an entry may sit idle and
/// whether one sighting is enough to keep it. A shell and a photo viewer want different answers to all three.
pub struct SoftwareRendererConfig {
    pub shadow: Policy,
    pub path_shadow: Policy,
    pub text_shadow: Policy,
    pub text_raster: Policy,
    pub text_shaping: Policy,
    pub font: renderer_core::FontConfig,
    /// The app wants a transparent surface. On Wayland this switches presentation from softbuffer (opaque XRGB) to an own `wl_shm` ARGB8888 buffer that preserves alpha; elsewhere it is currently a no-op (softbuffer stays opaque).
    pub transparent: bool,
}

impl Default for SoftwareRendererConfig {
    fn default() -> Self {
        Self {
            shadow: limits::SHADOW,
            path_shadow: limits::PATH_SHADOW,
            text_shadow: limits::TEXT_SHADOW,
            text_raster: limits::TEXT_RASTER,
            text_shaping: limits::TEXT_SHAPING,
            font: renderer_core::FontConfig::default(),
            transparent: false,
        }
    }
}
