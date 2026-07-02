use crate::Color;

use super::BorderRadius;
use super::paint::{FillRule, Paint, Shadow, Stroke};

pub trait ShapeStyle: Sized {
    fn fill_mut(&mut self) -> &mut Option<Paint>;
    fn stroke_mut(&mut self) -> &mut Option<Stroke>;
    fn shadow_mut(&mut self) -> &mut Option<Shadow>;

    fn with_fill(mut self, fill: impl Into<Paint>) -> Self {
        *self.fill_mut() = Some(fill.into());
        self
    }
    fn with_stroke(mut self, stroke: Stroke) -> Self {
        *self.stroke_mut() = Some(stroke);
        self
    }
    fn with_shadow(mut self, shadow: Shadow) -> Self {
        *self.shadow_mut() = Some(shadow);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub radius: BorderRadius,
}

impl RectStyle {
    pub fn filled(color: Color, radius: f32) -> Self {
        Self {
            fill: Some(Paint::Solid(color)),
            radius: BorderRadius::all(radius),
            ..Self::default()
        }
    }

    pub fn with_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }
}

impl ShapeStyle for RectStyle {
    fn fill_mut(&mut self) -> &mut Option<Paint> {
        &mut self.fill
    }
    fn stroke_mut(&mut self) -> &mut Option<Stroke> {
        &mut self.stroke
    }
    fn shadow_mut(&mut self) -> &mut Option<Shadow> {
        &mut self.shadow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub fill_rule: FillRule,
}

impl PathStyle {
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }
}

impl ShapeStyle for PathStyle {
    fn fill_mut(&mut self) -> &mut Option<Paint> {
        &mut self.fill
    }
    fn stroke_mut(&mut self) -> &mut Option<Stroke> {
        &mut self.stroke
    }
    fn shadow_mut(&mut self) -> &mut Option<Shadow> {
        &mut self.shadow
    }
}
