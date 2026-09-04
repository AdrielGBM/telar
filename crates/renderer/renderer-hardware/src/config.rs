//! The knobs a caller overrides to size the hardware backend for its own workload.

/// Cache bounds and pool sizes for the hardware renderer. Pass to `HardwareRenderer::new()` to override the compiled-in defaults without editing internal constants.
///
/// The cache bounds are [`Policy`](renderer_cache::Policy) values in the same units the software backend uses. They used to be frame counts with no size ceiling at all, which made a GPU cache's bound depend on refresh rate and left "how much VRAM may this hold" unanswerable.
#[derive(Clone, Copy)]
pub struct HardwareRendererConfig {
    /// The app wants a transparent surface: pick a premultiplied-alpha composite mode so the compositor blends it, instead of forcing Opaque.
    pub transparent: bool,
}

impl Default for HardwareRendererConfig {
    fn default() -> Self {
        Self { transparent: false }
    }
}
