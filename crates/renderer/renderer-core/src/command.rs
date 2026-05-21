use std::num::NonZeroU64;
use std::rc::Rc;

use crate::{
    ImageData, ImageFilter, LineStyle, PathData, PathStyle, Point, Rect, RectStyle, TextStyle,
};

#[derive(Debug, Clone)]
pub struct DrawNode {
    pub id: Option<NonZeroU64>,
    pub command: DrawCommand,
}

impl DrawNode {
    pub fn unkeyed(command: DrawCommand) -> Self {
        Self { id: None, command }
    }

    pub fn keyed(id: NonZeroU64, command: DrawCommand) -> Self {
        Self {
            id: Some(id),
            command,
        }
    }
}

impl From<DrawCommand> for DrawNode {
    fn from(command: DrawCommand) -> Self {
        Self::unkeyed(command)
    }
}

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
