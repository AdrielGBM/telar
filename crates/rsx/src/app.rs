use platform_core::Event;
use renderer_core::DrawCommand;

pub use platform_core::WindowConfig;
pub use renderer_core::{BorderRadius, Color, FillStyle, Rect, Stroke, TextStyle};

use crate::context::AppContext;

pub struct Frame {
    pub(crate) commands: Vec<DrawCommand>,
    pub(crate) clear_color: Option<Color>,
}

impl Frame {
    pub(crate) fn new() -> Self {
        Self {
            commands: Vec::new(),
            clear_color: None,
        }
    }

    pub fn clear(&mut self, color: Color) {
        self.clear_color = Some(color);
    }

    pub fn draw_rect(
        &mut self,
        rect: Rect,
        fill: Option<FillStyle>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
    ) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill,
            stroke,
            radius,
        });
    }

    pub fn draw_text(&mut self, text: &str, rect: Rect, style: TextStyle) {
        self.commands.push(DrawCommand::Text {
            text: text.to_owned(),
            rect,
            style,
        });
    }
}

pub trait App {
    fn on_resume(&mut self, _ctx: &mut AppContext) {}
    fn on_event(&mut self, _event: Event, _ctx: &mut AppContext) {}
    fn on_redraw(&mut self, frame: &mut Frame, ctx: &mut AppContext);
    fn on_suspend(&mut self, _ctx: &mut AppContext) {}
}
