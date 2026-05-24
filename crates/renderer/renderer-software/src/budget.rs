use crate::limits::*;

/// Per-cache memory budgets for the software renderer. Pass to `SoftwareRenderer::new()` to override the compiled-in defaults without editing internal constants.
pub struct RendererBudget {
    pub image_cache_bytes: usize,
    pub shadow_cache_bytes: usize,
    pub text_pixel_cache_bytes: usize,
    pub text_alpha_cache_bytes: usize,
    pub text_shaping_cache_bytes: usize,
    pub text_pixmap_cache_entries: usize,
}

impl Default for RendererBudget {
    fn default() -> Self {
        Self {
            image_cache_bytes: IMAGE_CACHE_BUDGET_BYTES,
            shadow_cache_bytes: SHADOW_CACHE_BUDGET_BYTES,
            text_pixel_cache_bytes: TEXT_PIXEL_CACHE_BUDGET_BYTES,
            text_alpha_cache_bytes: TEXT_ALPHA_CACHE_BUDGET_BYTES,
            text_shaping_cache_bytes: TEXT_SHAPING_CACHE_BUDGET_BYTES,
            text_pixmap_cache_entries: TEXT_PIXMAP_CACHE_MAX_ENTRIES,
        }
    }
}
