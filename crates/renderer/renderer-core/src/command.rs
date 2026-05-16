use std::sync::Arc;

use crate::{
    ImageData, ImageFilter, LineStyle, PathData, PathStyle, Point, Rect, RectStyle, TextStyle,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        style: RectStyle,
    },
    Text {
        text: String,
        rect: Rect,
        style: TextStyle,
    },
    Image {
        data: Arc<ImageData>,
        rect: Rect,
        filter: ImageFilter,
    },
    Line {
        p1: Point,
        p2: Point,
        style: LineStyle,
    },
    Path {
        data: Arc<PathData>,
        style: PathStyle,
    },
}
