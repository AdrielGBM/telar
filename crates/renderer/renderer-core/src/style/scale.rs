use geometry_core::Point;

use super::gradient::{Gradient, GradientKind};
use super::paint::{Paint, Shadow, Stroke};
use super::shape::{PathStyle, RectStyle};
use super::{BorderRadius, Scale, TextStyle};

impl Scale for Paint {
    fn scale(self, sf: f32) -> Self {
        match self {
            Paint::Solid(_) => self,
            Paint::Gradient(g) => Paint::Gradient(Gradient {
                kind: match g.kind {
                    GradientKind::Linear { start, end } => GradientKind::Linear {
                        start: Point::new(start.x * sf, start.y * sf),
                        end: Point::new(end.x * sf, end.y * sf),
                    },
                    GradientKind::Radial { center, radius } => GradientKind::Radial {
                        center: Point::new(center.x * sf, center.y * sf),
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

impl Scale for RectStyle {
    fn scale(self, sf: f32) -> Self {
        RectStyle {
            fill: self.fill.map(|p| p.scale(sf)),
            stroke: self.stroke.map(|s| s.scale(sf)),
            shadow: self.shadow.map(|s| s.scale(sf)),
            radius: self.radius.scale(sf),
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
