use crate::{BorderRadius, FillStyle, Rect, Stroke, TextStyle};

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
}
