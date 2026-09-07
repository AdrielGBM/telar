//! Gradient paint: the stops, and the linear and radial forms a backend resolves them into.

use geometry_core::Point;

use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
/// One stop: a colour and its position along the ramp, in `0.0..=1.0`.
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}

/// Up to 8 gradient color stops. Fixed-size array preserves the `Copy` bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStops {
    stops: [GradientStop; 8],
    count: u8,
}

impl GradientStops {
    pub fn new(stops: &[(f32, Color)]) -> Self {
        debug_assert!(
            stops.len() <= 8,
            "gradient has {} stops, max is 8",
            stops.len()
        );
        let count = stops.len().min(8) as u8;
        let mut arr = [GradientStop::new(0.0, Color::TRANSPARENT); 8];
        for (i, &(position, color)) in stops.iter().take(8).enumerate() {
            arr[i] = GradientStop::new(position, color);
        }
        Self { stops: arr, count }
    }

    pub fn active(&self) -> &[GradientStop] {
        &self.stops[..self.count as usize]
    }

    /// Every stop at a fraction of the opacity it already had.
    pub fn faded(mut self, factor: f32) -> Self {
        for stop in &mut self.stops[..self.count as usize] {
            stop.color = stop.color.with_alpha(stop.color.a * factor);
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Linear or radial, and the geometry each is defined by.
pub enum GradientKind {
    Linear { start: Point, end: Point },
    Radial { center: Point, radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// A gradient paint: its stops and the shape they are laid along.
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: GradientStops,
}

impl Gradient {
    pub fn linear(start: Point, end: Point, stops: &[(f32, Color)]) -> Self {
        Self {
            kind: GradientKind::Linear { start, end },
            stops: GradientStops::new(stops),
        }
    }

    pub fn radial(center: Point, radius: f32, stops: &[(f32, Color)]) -> Self {
        Self {
            kind: GradientKind::Radial { center, radius },
            stops: GradientStops::new(stops),
        }
    }
}

#[cfg(test)]
#[path = "gradient_test.rs"]
mod tests;
