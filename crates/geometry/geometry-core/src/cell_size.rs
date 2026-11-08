/// How many logical pixels one terminal cell stands for.
///
/// The terminal has no pixels, but every layout value in a Telar app is written in them — `pad:24`, `width:300`, a theme's `gutter`. Rather than teach the layout engine a second unit, the terminal declares an exchange rate and reports its own size in the same currency: an 80×24 terminal at the default rate is a 640×384 "window". Everything above stays exactly as it is on the desktop, and the rounding to whole cells happens once, in the painter, on absolute edges.
///
/// The default is the proportion of a typical monospace face — twice as tall as it is wide — which is what makes a layout authored for the desktop come out with recognisable proportions here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

impl Default for CellSize {
    fn default() -> Self {
        Self {
            width: 8.0,
            height: 16.0,
        }
    }
}

impl CellSize {
    /// The column a logical x falls in. Rounds, and callers round *both* edges of a box rather than its width, so two boxes that touch in logical space still touch in cells.
    ///
    /// Half **up** rather than half away from zero, so that adding a whole number of cells to a coordinate adds the same whole number of columns to the answer. `f32::round` changes direction at the origin and breaks that for anything scrolled past it — see [`LayoutGrid`](crate::LayoutGrid).
    pub fn col_at(self, x: f32) -> i32 {
        (x / self.width + 0.5).floor() as i32
    }

    pub fn row_at(self, y: f32) -> i32 {
        (y / self.height + 0.5).floor() as i32
    }

    pub fn cols_in(self, width: f32) -> u16 {
        (width / self.width).floor().clamp(0.0, u16::MAX as f32) as u16
    }
}

#[cfg(test)]
#[path = "cell_size.test.rs"]
mod tests;
