use platform_core::Event;
use renderer_core::RenderBackend;

pub use platform_core::WindowConfig;
pub use renderer_core::{BorderRadius, Color, Rect, Stroke};

use crate::context::AppContext;

pub struct Frame<'a> {
    pub(crate) renderer: &'a mut dyn RenderBackend,
}

impl<'a> Frame<'a> {
    pub fn clear(&mut self, color: Color) {
        self.renderer.clear(color);
    }

    pub fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        self.renderer.draw_rect(rect, fill, stroke, radius);
    }
}

pub trait App {
    fn on_resume(&mut self, _ctx: &mut AppContext) {}
    fn on_event(&mut self, _event: Event, _ctx: &mut AppContext) {}
    fn on_redraw(&mut self, frame: &mut Frame, ctx: &mut AppContext);
    fn on_suspend(&mut self, _ctx: &mut AppContext) {}
}
