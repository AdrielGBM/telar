use geometry_core::{Point, Rect};

use super::gradient::{Gradient, GradientKind};
use super::paint::{Paint, Shadow, Stroke};
use super::shape::{BorderWidths, PathStyle, RectStyle};
use super::{Declared, Scale, Span, TextStyle};
use crate::BorderRadius;

// `Scale` is local to renderer-core, so the orphan rules permit implementing it for these foreign value types here rather than adding arithmetic to the geometry-core crate.
impl Scale for Point {
    fn scale(self, sf: f32) -> Self {
        Point::new(self.x * sf, self.y * sf)
    }
}

impl Scale for Rect {
    fn scale(self, sf: f32) -> Self {
        Rect::new(self.x * sf, self.y * sf, self.width * sf, self.height * sf)
    }
}

impl Scale for Paint {
    fn scale(self, sf: f32) -> Self {
        match self {
            Paint::Solid(_) => self,
            Paint::Gradient(g) => Paint::Gradient(Gradient {
                kind: match g.kind {
                    GradientKind::Linear { start, end } => GradientKind::Linear {
                        start: start.scale(sf),
                        end: end.scale(sf),
                    },
                    GradientKind::Radial { center, radius } => GradientKind::Radial {
                        center: center.scale(sf),
                        radius: radius * sf,
                    },
                },
                stops: g.stops,
            }),
        }
    }
}

impl Scale for Stroke {
    fn scale(self, sf: f32) -> Self {
        Stroke {
            paint: self.paint.scale(sf),
            width: self.width * sf,
            cap: self.cap,
            join: self.join,
        }
    }
}

impl Scale for Shadow {
    fn scale(self, sf: f32) -> Self {
        Shadow {
            offset_x: self.offset_x * sf,
            offset_y: self.offset_y * sf,
            blur_radius: self.blur_radius * sf,
            spread: self.spread * sf,
            color: self.color,
        }
    }
}

impl Scale for BorderRadius {
    fn scale(self, sf: f32) -> Self {
        BorderRadius {
            top_left: self.top_left * sf,
            top_right: self.top_right * sf,
            bottom_right: self.bottom_right * sf,
            bottom_left: self.bottom_left * sf,
        }
    }
}

impl Scale for BorderWidths {
    fn scale(self, sf: f32) -> Self {
        match self {
            // Uniform carries no number of its own: it defers to the stroke, which scales itself.
            BorderWidths::Uniform => self,
            BorderWidths::PerSide {
                top,
                right,
                bottom,
                left,
            } => BorderWidths::PerSide {
                top: top * sf,
                right: right * sf,
                bottom: bottom * sf,
                left: left * sf,
            },
        }
    }
}

impl Scale for RectStyle {
    fn scale(self, sf: f32) -> Self {
        RectStyle {
            fill: self.fill.map(|p| p.scale(sf)),
            stroke: self.stroke.map(|s| s.scale(sf)),
            shadow: self.shadow.map(|s| s.scale(sf)),
            radius: self.radius.scale(sf),
            border_widths: self.border_widths.scale(sf),
        }
    }
}

impl Scale for PathStyle {
    fn scale(self, sf: f32) -> Self {
        PathStyle {
            fill: self.fill.map(|p| p.scale(sf)),
            stroke: self.stroke.map(|s| s.scale(sf)),
            shadow: self.shadow.map(|s| s.scale(sf)),
            fill_rule: self.fill_rule,
        }
    }
}

impl Scale for TextStyle {
    fn scale(self, sf: f32) -> Self {
        TextStyle {
            font_size: self.font_size * sf,
            paint: self.paint.scale(sf),
            shadow: self.shadow.map(|s| s.scale(sf)),
            // letter_spacing is a pixel advance, so it scales with font_size; line_height is a unitless multiple and rides along via `..self`.
            letter_spacing: self.letter_spacing * sf,
            ..self
        }
    }
}

impl Scale for Declared {
    fn scale(self, sf: f32) -> Self {
        Declared {
            font_size: self.font_size.map(|v| v * sf),
            paint: self.paint.map(|p| p.scale(sf)),
            letter_spacing: self.letter_spacing.map(|v| v * sf),
            ..self
        }
    }
}

impl Scale for Span {
    fn scale(self, sf: f32) -> Self {
        Span {
            over: self.over.scale(sf),
            ..self
        }
    }
}
