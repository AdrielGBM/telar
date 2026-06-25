use crate::preprocess::{blur_padding, blur_sigma};

pub struct ShadowLayout {
    pub sigma: f32,
    pub padding: i32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub texture_width_logical: u32,
    pub texture_height_logical: u32,
    pub texture_width: u32,
    pub texture_height: u32,
}

impl ShadowLayout {
    pub fn compute(
        blur_radius: f32,
        world_min_x: f32,
        world_max_x: f32,
        world_min_y: f32,
        world_max_y: f32,
        scale_factor: f32,
    ) -> Self {
        // Padding is derived from the logical sigma so the texture margin matches the pre-scale geometry; `sigma` holds the physical sigma used by the blur pass.
        let logical_sigma = blur_sigma(blur_radius);
        let padding = blur_padding(logical_sigma);
        let sigma = logical_sigma * scale_factor;
        let origin_x = world_min_x - padding as f32;
        let origin_y = world_min_y - padding as f32;
        let texture_width_logical =
            ((world_max_x - world_min_x).ceil() + 2.0 * padding as f32).max(1.0) as u32;
        let texture_height_logical =
            ((world_max_y - world_min_y).ceil() + 2.0 * padding as f32).max(1.0) as u32;
        let texture_width = (texture_width_logical as f32 * scale_factor).ceil() as u32;
        let texture_height = (texture_height_logical as f32 * scale_factor).ceil() as u32;
        Self {
            sigma,
            padding,
            origin_x,
            origin_y,
            texture_width_logical,
            texture_height_logical,
            texture_width,
            texture_height,
        }
    }
}
