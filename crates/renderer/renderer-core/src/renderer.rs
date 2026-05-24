use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError>;
    /// Process and present all draw commands for this frame. Must be called exactly once per frame after `begin_frame`.
    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError>;
}
