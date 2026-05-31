use std::rc::Rc;

use geometry_core::{Point, Rect};

use crate::{
    BorderRadius, ImageData, ImageFilter, LineStyle, PathData, PathStyle, RectStyle, TextStyle,
};

// Boxed to keep DrawCommand at 40 bytes; RectStyle is ~180 bytes on its own and would otherwise dominate the enum size, hurting cache utilization across the command buffer.
#[derive(Debug, Clone)]
pub struct RectPayload {
    pub rect: Rect,
    pub style: RectStyle,
}

#[derive(Debug, Clone)]
pub struct TextPayload {
    pub text: Rc<str>,
    pub rect: Rect,
    pub style: TextStyle,
}

#[derive(Debug, Clone)]
pub struct PathPayload {
    pub data: Rc<PathData>,
    pub style: PathStyle,
}

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect(Box<RectPayload>),
    Text(Box<TextPayload>),
    Image {
        data: Rc<ImageData>,
        rect: Rect,
        filter: ImageFilter,
    },
    Line {
        p1: Point,
        p2: Point,
        style: LineStyle,
    },
    Path(Box<PathPayload>),
    PushClip {
        rect: Rect,
        radius: BorderRadius,
    },
    PopClip,
    PushMatrix {
        matrix: [f32; 6],
    },
    PopMatrix,
    PushLayer {
        opacity: f32,
        backdrop_blur: f32,
    },
    PopLayer,
}

impl PartialEq for RectPayload {
    fn eq(&self, other: &Self) -> bool {
        self.rect == other.rect && self.style == other.style
    }
}

impl PartialEq for TextPayload {
    fn eq(&self, other: &Self) -> bool {
        *self.text == *other.text && self.rect == other.rect && self.style == other.style
    }
}

impl PartialEq for PathPayload {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.data, &other.data) && self.style == other.style
    }
}

impl PartialEq for DrawCommand {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DrawCommand::Rect(a), DrawCommand::Rect(b)) => a == b,
            (DrawCommand::Text(a), DrawCommand::Text(b)) => a == b,
            (
                DrawCommand::Image {
                    data: d1,
                    rect: r1,
                    filter: f1,
                },
                DrawCommand::Image {
                    data: d2,
                    rect: r2,
                    filter: f2,
                },
            ) => d1.id == d2.id && r1 == r2 && f1 == f2,
            (
                DrawCommand::Line {
                    p1: p1a,
                    p2: p2a,
                    style: s1,
                },
                DrawCommand::Line {
                    p1: p1b,
                    p2: p2b,
                    style: s2,
                },
            ) => p1a == p1b && p2a == p2b && s1 == s2,
            (DrawCommand::Path(a), DrawCommand::Path(b)) => a == b,
            (
                DrawCommand::PushClip {
                    rect: r1,
                    radius: br1,
                },
                DrawCommand::PushClip {
                    rect: r2,
                    radius: br2,
                },
            ) => r1 == r2 && br1 == br2,
            (DrawCommand::PopClip, DrawCommand::PopClip) => true,
            (DrawCommand::PushMatrix { matrix: m1 }, DrawCommand::PushMatrix { matrix: m2 }) => {
                m1 == m2
            }
            (DrawCommand::PopMatrix, DrawCommand::PopMatrix) => true,
            (
                DrawCommand::PushLayer {
                    opacity: o1,
                    backdrop_blur: b1,
                },
                DrawCommand::PushLayer {
                    opacity: o2,
                    backdrop_blur: b2,
                },
            ) => o1 == o2 && b1 == b2,
            (DrawCommand::PopLayer, DrawCommand::PopLayer) => true,
            _ => false,
        }
    }
}
