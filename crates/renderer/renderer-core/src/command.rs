use std::rc::Rc;

use geometry_core::{Point, Rect};

use crate::{ImageData, ImageFilter, LineStyle, PathData, PathStyle, RectStyle, TextStyle};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        style: RectStyle,
    },
    Text {
        text: Rc<str>,
        rect: Rect,
        style: TextStyle,
    },
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
    Path {
        data: Rc<PathData>,
        style: PathStyle,
    },
    PushClip {
        rect: Rect,
    },
    PopClip,
    PushTransform {
        tx: f32,
        ty: f32,
    },
    PopTransform,
    PushLayer {
        opacity: f32,
    },
    PopLayer,
}

impl PartialEq for DrawCommand {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                DrawCommand::Rect {
                    rect: r1,
                    style: s1,
                },
                DrawCommand::Rect {
                    rect: r2,
                    style: s2,
                },
            ) => r1 == r2 && s1 == s2,
            (
                DrawCommand::Text {
                    text: t1,
                    rect: r1,
                    style: s1,
                },
                DrawCommand::Text {
                    text: t2,
                    rect: r2,
                    style: s2,
                },
            ) => **t1 == **t2 && r1 == r2 && s1 == s2,
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
            (
                DrawCommand::Path {
                    data: d1,
                    style: s1,
                },
                DrawCommand::Path {
                    data: d2,
                    style: s2,
                },
            ) => Rc::ptr_eq(d1, d2) && s1 == s2,
            (DrawCommand::PushClip { rect: r1 }, DrawCommand::PushClip { rect: r2 }) => r1 == r2,
            (DrawCommand::PopClip, DrawCommand::PopClip) => true,
            (
                DrawCommand::PushTransform { tx: tx1, ty: ty1 },
                DrawCommand::PushTransform { tx: tx2, ty: ty2 },
            ) => tx1 == tx2 && ty1 == ty2,
            (DrawCommand::PopTransform, DrawCommand::PopTransform) => true,
            (DrawCommand::PushLayer { opacity: o1 }, DrawCommand::PushLayer { opacity: o2 }) => {
                o1 == o2
            }
            (DrawCommand::PopLayer, DrawCommand::PopLayer) => true,
            _ => false,
        }
    }
}
