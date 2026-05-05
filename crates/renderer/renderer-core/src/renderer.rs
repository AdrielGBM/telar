use crate::Color;

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn clear(&mut self, color: Color);
    fn end_frame(&mut self);
}
