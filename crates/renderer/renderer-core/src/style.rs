use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
}

impl TextStyle {
    pub fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillStyle {
    Solid(Color),
}

impl FillStyle {
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Solid(c) => *c,
        }
    }
}

impl From<Color> for FillStyle {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineStyle {
    pub color: Color,
    pub width: f32,
    pub cap: LineCap,
}

impl LineStyle {
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            cap: LineCap::Butt,
        }
    }
}
