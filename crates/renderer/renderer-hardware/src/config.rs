use crate::limits::*;

/// Cache ages and pool sizes for the hardware renderer. Pass to `HardwareRenderer::new()` to override the compiled-in defaults without editing internal constants.
#[derive(Clone, Copy)]
pub struct HardwareRendererConfig {
    pub image_gpu_max_age_frames: u64,
    pub path_tess_max_age_frames: u64,
    pub viewport_pool_size: usize,
    pub max_texture_pool_per_size: usize,
}

impl Default for HardwareRendererConfig {
    fn default() -> Self {
        Self {
            image_gpu_max_age_frames: IMAGE_GPU_MAX_AGE_FRAMES,
            path_tess_max_age_frames: PATH_TESS_MAX_AGE_FRAMES,
            viewport_pool_size: VIEWPORT_POOL_SIZE,
            max_texture_pool_per_size: MAX_TEXTURE_POOL_PER_SIZE,
        }
    }
}
