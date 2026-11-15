//! [`SurfaceConventions`]: what the platform showing a surface says about it, which the tree on that surface lays out and takes input by.

use geometry_core::LayoutGrid;

use crate::{FocusBoundary, KeyboardConvention};

/// How the platform a surface is shown on works, as far as the tree on it has to know. Reported by the platform, handed to each surface it shows before the tree on it is built, and carried to a guest drawn inside one with the surface it is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceConventions {
    /// The smallest step the surface can resolve: [`LayoutGrid::UNIT`] for pixels, a cell for a terminal.
    pub grid: LayoutGrid,
    /// Whose keyboard the person at the surface has, which decides what `Mod` in a shortcut is and how a shortcut is shown.
    pub keyboard: KeyboardConvention,
    /// Whether a wheel notch glides to where it scrolls to rather than landing in one frame: off where the smallest visible step is a whole cell, since easing across one would repaint the same rows several times.
    pub smooth_wheel: bool,
    /// What Tab past the last focusable, or Shift+Tab past the first, does on the surface.
    pub focus_boundary: FocusBoundary,
}

/// A window of the system this was compiled for: pixels, its keyboard, a gliding wheel and a focus order that loops.
impl Default for SurfaceConventions {
    fn default() -> Self {
        Self {
            grid: LayoutGrid::UNIT,
            keyboard: KeyboardConvention::native(),
            smooth_wheel: true,
            focus_boundary: FocusBoundary::Wrap,
        }
    }
}
