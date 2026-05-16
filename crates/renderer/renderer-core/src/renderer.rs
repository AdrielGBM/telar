use crate::{Color, DrawCommand, RendererError};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError>;
    /// Submit draw commands for the current frame. Must be called exactly once per frame, between `begin_frame` and `end_frame`. Calling it multiple times per frame produces undefined ordering and batching behavior across backends.
    fn submit(&mut self, commands: &[DrawCommand]);
    fn end_frame(&mut self, clear_color: Option<Color>) -> Result<(), RendererError>;
}
