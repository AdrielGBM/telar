//! The knobs a caller overrides for the hardware backend.

/// What a caller may set on the hardware renderer. Pass to `HardwareRenderer::new()`. Its caches take their bounds from [`renderer_cache::limits`].
#[derive(Clone, Copy, Default)]
pub struct HardwareRendererConfig {
    /// The app wants a transparent surface: pick a premultiplied-alpha composite mode so the compositor blends it, instead of forcing Opaque.
    pub transparent: bool,
}
