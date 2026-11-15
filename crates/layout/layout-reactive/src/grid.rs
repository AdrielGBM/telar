//! The step each surface resolves to, as surface state: declared by whatever hosts the tree before it is built, and read by the layout pass, which puts every definite length on it.
//!
//! Per surface, because a terminal's cells and a window's pixels can share one thread: a canvas drawn in a window resolves anywhere while the terminal beside it resolves to whole cells.

use geometry_core::LayoutGrid;

reactive_core::surface_local! {
    /// The step the active surface resolves to.
    slot GRID: LayoutGrid = LayoutGrid::UNIT;
    access with_grid, with_grid_ref;
    context LayoutGridContext;
}

/// Declares the step the active surface resolves to. Whatever hosts the tree calls this before building it; the next layout pass resolves every node on it.
pub fn set_layout_grid(grid: LayoutGrid) {
    with_grid(|current| *current = grid);
}

/// The step the active surface resolves to, [`LayoutGrid::UNIT`] until one is declared.
pub fn layout_grid() -> LayoutGrid {
    with_grid_ref(|grid| *grid)
}

#[cfg(test)]
#[path = "grid.test.rs"]
mod tests;
