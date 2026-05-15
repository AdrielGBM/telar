use std::sync::Arc;

use crate::{
    BorderRadius, FillRule, FillStyle, ImageData, ImageFilter, LineStyle, PathData, Point, Rect,
    Stroke, TextStyle,
};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        fill: Option<FillStyle>,
        stroke: Option<Stroke>,
        radius: BorderRadius,
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
        fill: Option<FillStyle>,
        stroke: Option<Stroke>,
        fill_rule: FillRule,
    },
}
