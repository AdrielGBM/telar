use crate::{BorderRadius, Color, FillStyle, Rect, Stroke, TextStyle};

pub trait RenderBackend {
    fn begin_frame(&mut self, width: u32, height: u32);
    fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<FillStyle>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    );
    fn draw_text(&mut self, text: &str, rect: Rect, style: TextStyle);
    fn end_frame(&mut self, clear_color: Option<Color>);
}
