//! The step a surface can actually resolve.
//!
//! Every layout value in a Telar app is written in logical pixels, which assumes a surface that can put an edge anywhere. A terminal cannot: it has character cells, and an edge lands on a cell boundary or it lands on the wrong one. [`CellSize`](https://docs.rs/telar-renderer-tui) declares the exchange rate so layout runs unchanged, but an exchange rate alone leaves boxes whose height is a fraction of a cell — and a fractional box does not have a size, it has a *size that depends on where it is*.
//!
//! The arithmetic is why. A backend that quantises maps an edge with `round(v / step)`, and rounds both edges of a box independently so two boxes sharing an edge share a cell instead of leaving a seam. The rows a box covers are then
//!
//! ```text
//! round((y + h) / step) - round(y / step)
//! ```
//!
//! which is a function of `y` as well as `h`. Only when `h` is a whole number of steps does it reduce to `h / step` for every `y` — so a box that is 2.3 cells tall is drawn 2 cells tall at one scroll offset and 3 at the next, and two identical boxes at different heights on the page disagree while nothing is moving at all.
//!
//! So the grid is declared once by the frontend and honoured where sizes are *authored*, not patched up where they are drawn. A surface that can put an edge anywhere leaves it at [`LayoutGrid::UNIT`], where every operation here is the identity and nothing downstream changes.

use std::sync::atomic::{AtomicU32, Ordering};

/// The smallest step a surface can resolve on each axis, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutGrid {
    pub x: f32,
    pub y: f32,
}

impl Default for LayoutGrid {
    fn default() -> Self {
        Self::UNIT
    }
}

impl LayoutGrid {
    /// A surface that can put an edge anywhere. Every snap is the identity.
    pub const UNIT: Self = Self { x: 1.0, y: 1.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: if x.is_finite() && x > 0.0 { x } else { 1.0 },
            y: if y.is_finite() && y > 0.0 { y } else { 1.0 },
        }
    }

    /// Whether snapping can be skipped entirely.
    pub fn is_unit(self) -> bool {
        self.x == 1.0 && self.y == 1.0
    }

    /// A length that *is* an element — a width, a height. Rounds to the nearest step, except that anything strictly positive keeps at least one: quantisation may not delete a box the author asked for.
    pub fn snap_size_x(self, v: f32) -> f32 {
        snap_size(v, self.x)
    }

    pub fn snap_size_y(self, v: f32) -> f32 {
        snap_size(v, self.y)
    }

    /// A length that is *air* around elements — padding, a gap. Kept only if it nearly fills a step; otherwise it goes to zero. See [`SPACE_KEPT_ABOVE`].
    pub fn snap_space_x(self, v: f32) -> f32 {
        snap_space(v, self.x)
    }

    pub fn snap_space_y(self, v: f32) -> f32 {
        snap_space(v, self.y)
    }

    /// A position: where something sits, rather than how big it is. Rounds to the nearest step in both directions — a coordinate has no minimum to protect, and negatives (a scrolled-away origin) round the same way.
    pub fn snap_pos_x(self, v: f32) -> f32 {
        snap_to(v, self.x)
    }

    pub fn snap_pos_y(self, v: f32) -> f32 {
        snap_to(v, self.y)
    }
}

/// Rounds half **up**, not half away from zero, and that is the whole of why this is not `f32::round`.
///
/// The invariant everything here rests on is that a whole number of steps stays a whole number of steps wherever it is put: `snap(y + h) - snap(y) == h` for `h` a multiple of `step`. That needs the rounding to commute with adding an integer, and half-away-from-zero does not — it changes direction at the origin, so `round(-2.5) = -3` while `round(53.5) = 54`, and a 56-cell box straddling zero comes out 57 cells tall. Which is precisely the case scrolling produces: a box pushed above the viewport has a negative `y`.
fn snap_to(v: f32, step: f32) -> f32 {
    if step == 1.0 || !v.is_finite() {
        return v;
    }
    (v / step + 0.5).floor() * step
}

fn snap_size(v: f32, step: f32) -> f32 {
    if step == 1.0 || !v.is_finite() || v <= 0.0 {
        return v;
    }
    snap_to(v, step).max(step)
}

/// How much of a step a piece of *space* has to fill before it is worth a whole one.
///
/// Not the midpoint, deliberately. Space is the one quantity where rounding up and rounding down are not symmetric mistakes: rounded down it leaves the layout tighter than it was drawn, while rounded up it spends the surface's scarcest axis — a terminal has tens of rows against hundreds of columns — on air the author asked half a row for. So air has to nearly fill a step to earn one.
///
/// Three quarters rather than some other bias because it is an exact fraction of the step, so the rule reads the same on both axes instead of landing between pixels on the narrower one: 12 of 16 keeps a row, 6 of 8 keeps a column, and 11 and 5 respectively collapse.
///
/// Sizes do not use it. A box is the thing itself rather than the space around it, so it rounds to the nearest step and never below one — quantisation may not delete what the author asked for.
pub const SPACE_KEPT_ABOVE: f32 = 0.75;

fn snap_space(v: f32, step: f32) -> f32 {
    if step == 1.0 || !v.is_finite() || v <= 0.0 {
        return v;
    }
    let steps = v / step;
    let whole = steps.floor();
    if steps - whole >= SPACE_KEPT_ABOVE {
        (whole + 1.0) * step
    } else {
        whole * step
    }
}

// Two f32 bit patterns rather than a lock: this is read on every layout value a tree builds, and a frontend writes it once before the first component exists.
static GRID_X: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
static GRID_Y: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

/// Declares the step this surface resolves to. A frontend calls this once, before the tree is built, alongside whatever text measurer it installs.
pub fn set_layout_grid(grid: LayoutGrid) {
    GRID_X.store(grid.x.to_bits(), Ordering::Relaxed);
    GRID_Y.store(grid.y.to_bits(), Ordering::Relaxed);
}

pub fn layout_grid() -> LayoutGrid {
    LayoutGrid {
        x: f32::from_bits(GRID_X.load(Ordering::Relaxed)),
        y: f32::from_bits(GRID_Y.load(Ordering::Relaxed)),
    }
}

#[cfg(test)]
#[path = "grid_test.rs"]
mod tests;
