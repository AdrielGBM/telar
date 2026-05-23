use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError>;
    /// Submit all draw commands for this frame. Must be called exactly once per frame.
    fn submit(&mut self, commands: &[DrawCommand]) -> Result<(), RendererError>;
    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError>;
}
