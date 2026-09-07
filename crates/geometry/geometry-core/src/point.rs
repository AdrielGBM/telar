//! A 2D point, and the arithmetic a transform needs on one.

/// A 2D point in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[cfg(test)]
#[path = "point_test.rs"]
mod tests;
