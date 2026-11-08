//! Gradient paint: the stops, and the linear and radial forms a backend resolves them into.

use geometry_core::{Point, Transform};

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

/// Up to [`GradientStops::CAPACITY`] gradient color stops. Fixed-size array preserves the `Copy` bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStops {
    stops: [GradientStop; GradientStops::CAPACITY],
    count: u8,
}

impl GradientStops {
    pub const CAPACITY: usize = 8;

    pub fn new(stops: &[(f32, Color)]) -> Self {
        debug_assert!(
            stops.len() <= Self::CAPACITY,
            "gradient has {} stops, max is {}",
            stops.len(),
            Self::CAPACITY
        );
        let count = stops.len().min(Self::CAPACITY) as u8;
        let mut arr = [GradientStop::new(0.0, Color::TRANSPARENT); Self::CAPACITY];
        for (i, &(position, color)) in stops.iter().take(Self::CAPACITY).enumerate() {
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
    Linear {
        start: Point,
        end: Point,
    },
    /// `transform` carries the unit circle onto the ellipse the last stop lies on: its translation is the centre and its linear part the radii, so whatever stretches or skews the drawing stretches the gradient with it.
    Radial {
        transform: Transform,
    },
}

impl GradientKind {
    /// The same geometry carried through `transform`.
    pub fn transformed(self, transform: Transform) -> Self {
        match self {
            GradientKind::Linear { start, end } => GradientKind::Linear {
                start: transform.apply(start),
                end: transform.apply(end),
            },
            GradientKind::Radial { transform: unit } => GradientKind::Radial {
                transform: unit.then(transform),
            },
        }
    }
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

    /// A circular radial gradient, reaching its last stop `radius` from `center`.
    pub fn radial(center: Point, radius: f32, stops: &[(f32, Color)]) -> Self {
        let circle = Transform {
            a: radius,
            b: 0.0,
            c: 0.0,
            d: radius,
            e: center.x,
            f: center.y,
        };
        Self::elliptical(circle, stops)
    }

    /// A radial gradient over the ellipse `transform` carries the unit circle onto.
    pub fn elliptical(transform: Transform, stops: &[(f32, Color)]) -> Self {
        Self {
            kind: GradientKind::Radial { transform },
            stops: GradientStops::new(stops),
        }
    }

    /// The same gradient with its geometry carried through `transform`.
    pub fn transformed(self, transform: Transform) -> Self {
        Self {
            kind: self.kind.transformed(transform),
            stops: self.stops,
        }
    }
}

#[cfg(test)]
#[path = "gradient.test.rs"]
mod tests;
