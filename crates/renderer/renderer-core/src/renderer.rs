use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError>;
    fn submit(&mut self, commands: &[DrawCommand]);
    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError>;
}
