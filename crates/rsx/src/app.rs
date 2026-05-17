use std::sync::Arc;

use platform_core::Event;
use renderer_core::{
    Color, DrawCommand, ImageData, ImageFilter, LineStyle, PathData, PathStyle, Point, Rect,
    RectStyle, TextStyle,
};

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

    pub fn draw_rect(&mut self, rect: Rect, style: RectStyle) {
        self.commands.push(DrawCommand::Rect { rect, style });
    }

    pub fn draw_text(&mut self, text: &str, rect: Rect, style: TextStyle) {
        self.commands.push(DrawCommand::Text {
            text: Arc::from(text),
            rect,
            style,
        });
    }

    pub fn draw_image(&mut self, data: Arc<ImageData>, rect: Rect, filter: ImageFilter) {
        self.commands
            .push(DrawCommand::Image { data, rect, filter });
    }

    pub fn draw_line(&mut self, p1: Point, p2: Point, style: LineStyle) {
        self.commands.push(DrawCommand::Line { p1, p2, style });
    }

    pub fn draw_path(&mut self, data: std::sync::Arc<PathData>, style: PathStyle) {
        self.commands.push(DrawCommand::Path { data, style });
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::PushClip { rect });
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    pub fn push_translate(&mut self, tx: f32, ty: f32) {
        self.commands.push(DrawCommand::PushTransform { tx, ty });
    }

    pub fn pop_transform(&mut self) {
        self.commands.push(DrawCommand::PopTransform);
    }
}

pub trait App {
    fn on_resume(&mut self, _ctx: &mut AppContext) {}
    fn on_event(&mut self, _event: Event, _ctx: &mut AppContext) {}
    fn on_redraw(&mut self, frame: &mut Frame, ctx: &mut AppContext);
    fn on_suspend(&mut self, _ctx: &mut AppContext) {}
}
