use std::sync::Arc;

use crate::{BorderRadius, FillStyle, ImageData, ImageFilter, Rect, Stroke, TextStyle};

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
}
