//! The knobs a caller overrides for the software backend.

/// Font config and presentation options for the software backend. Pass to `SoftwareRenderer::new()`. Its caches take their bounds from [`renderer_cache::limits`].
#[derive(Default)]
pub struct SoftwareRendererConfig {
    /// How text is shaped and which fonts it may fall back to.
    pub font: renderer_core::FontConfig,
    /// The app wants a transparent surface. On Wayland this switches presentation from softbuffer (opaque XRGB) to an own `wl_shm` ARGB8888 buffer that preserves alpha; elsewhere it is currently a no-op (softbuffer stays opaque).
    pub transparent: bool,
    /// The window system keeps what was last presented on screen, so a present declares only the regions that changed. Leave it `false` unless the platform guarantees that: on a surface that does not keep its contents, a region another window uncovers would never be repainted.
    pub retains_presented_contents: bool,
    /// Blur every shadow on the calling thread, however large, instead of handing the big ones to a worker that lands its result a frame or two later. For an offscreen renderer whose frames are read back whole, such as a snapshot or an exported image: it has no later frame to land on, so an asynchronous shadow would be missing from the first picture. Costs the frame time of the blur.
    pub synchronous_shadows: bool,
}
