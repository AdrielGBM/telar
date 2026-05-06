use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{BorderRadius, Color, Rect, Stroke};

use crate::renderer::{HardwareRenderer, RectInstance};

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub(crate) fn draw_rect_impl(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        let fill_color = match fill {
            Some(c) => [c.r, c.g, c.b, c.a],
            None => [0.0; 4],
        };
        let (stroke_color, stroke_width) = match stroke {
            Some(s) => ([s.color.r, s.color.g, s.color.b, s.color.a], s.width),
            None => ([0.0; 4], 0.0),
        };
        self.rect_queue.push(RectInstance {
            rect: [rect.x, rect.y, rect.w, rect.h],
            radii: [
                radius.top_left,
                radius.top_right,
                radius.bottom_right,
                radius.bottom_left,
            ],
            fill_color,
            stroke_color,
            stroke_width,
            _pad: [0.0; 3],
        });
    }
}
