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
