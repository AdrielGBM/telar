use geometry_core::Point;

use crate::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    Linear { start: Point, end: Point },
    Radial { center: Point, radius: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
mod tests {
    use super::*;

    #[test]
    fn gradient_stops_new_stores_stops() {
        let stops = GradientStops::new(&[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
        assert_eq!(stops.active().len(), 2);
        assert_eq!(stops.active()[0].color, Color::BLACK);
        assert_eq!(stops.active()[1].color, Color::WHITE);
    }

    #[test]
    fn gradient_stops_new_truncates_to_eight() {
        let raw: Vec<(f32, Color)> = (0..6).map(|i| (i as f32 / 5.0, Color::BLACK)).collect();
        let stops = GradientStops::new(&raw);
        assert_eq!(stops.active().len(), 6);
    }

    #[test]
    fn gradient_linear_stores_stops() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        let g = Gradient::linear(p1, p2, &[(0.0, Color::BLACK), (1.0, Color::WHITE)]);
        assert_eq!(g.stops.active().len(), 2);
        assert_eq!(g.stops.active()[0].color, Color::BLACK);
    }

    #[test]
    fn gradient_radial_stores_stops() {
        let c = Point::new(0.5, 0.5);
        let g = Gradient::radial(c, 1.0, &[(0.0, Color::RED), (1.0, Color::TRANSPARENT)]);
        assert_eq!(g.stops.active().len(), 2);
        assert_eq!(g.stops.active()[0].color, Color::RED);
    }
}
