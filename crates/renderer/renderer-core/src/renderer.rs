use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    /// Begin a new frame. Note: `scale_factor` and `generation` may be ignored by backends that receive pre-scaled commands (see `SoftwareRenderer::begin_frame`).
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError>;
    /// Process and present all draw commands for this frame. Must be called exactly once per frame after `begin_frame`.
    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError>;
}
