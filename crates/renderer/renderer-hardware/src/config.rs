use crate::limits::*;
use renderer_cache::{Policy, limits};

/// Cache bounds and pool sizes for the hardware renderer. Pass to `HardwareRenderer::new()` to override the compiled-in defaults without editing internal constants.
///
/// The cache bounds are [`Policy`] values in the same units the software backend uses. They used to be frame counts
/// with no size ceiling at all, which made a GPU cache's bound depend on refresh rate and left "how much VRAM may
/// this hold" unanswerable.
#[derive(Clone, Copy)]
pub struct HardwareRendererConfig {
    /// Uploaded image textures. Defaults to the floor of the surface-derived budget and grows with the surface.
    pub image_texture: Policy,
    pub path_tess: Policy,
    pub shadow: Policy,
    pub viewport_pool_size: usize,
    pub max_texture_pool_per_size: usize,
    /// The app wants a transparent surface: pick a premultiplied-alpha composite mode so the compositor blends it, instead of forcing Opaque.
    pub transparent: bool,
}

impl Default for HardwareRendererConfig {
    fn default() -> Self {
        Self {
            image_texture: limits::gpu_texture(0, 0),
            path_tess: limits::GPU_PATH_TESS,
            shadow: limits::GPU_SHADOW,
            viewport_pool_size: VIEWPORT_POOL_SIZE,
            max_texture_pool_per_size: MAX_TEXTURE_POOL_PER_SIZE,
            transparent: false,
        }
    }
}
