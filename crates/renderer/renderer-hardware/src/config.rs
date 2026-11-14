//! The knobs a caller overrides for the hardware backend.

/// What a caller may set on the hardware renderer. Pass to `HardwareRenderer::new()`. Its caches take their bounds from [`renderer_cache::limits`].
#[derive(Clone, Copy, Default)]
pub struct HardwareRendererConfig {
    /// The app wants a transparent surface: pick a premultiplied-alpha composite mode so the compositor blends it, instead of forcing Opaque.
    pub transparent: bool,
    /// Whether frames may reuse what the one before drew, by repainting only what changed or moving scrolled pixels; a developer turns either off to A/B it.
    pub debug: renderer_core::DebugSettings,
}
