use crate::{BorderRadius, Color, Rect, Stroke};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn clear(&mut self, color: Color);
    fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    );
    fn end_frame(&mut self);
}
