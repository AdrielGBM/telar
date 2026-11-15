//! The [`SurfaceConventions`] a surface was put on, kept in that surface's own worlds.

use platform_core::SurfaceConventions;

/// Puts the active surface on `conventions`, what the platform showing it says about it. Whatever hosts a tree calls this before building it, as a runner does for its window and a canvas for its own surface.
pub fn set_surface_conventions(conventions: SurfaceConventions) {
    let SurfaceConventions {
        grid,
        keyboard,
        smooth_wheel,
        focus_boundary,
    } = conventions;
    layout_reactive::set_layout_grid(grid);
    platform_core::set_keyboard_convention(keyboard);
    ui_tree::set_smooth_wheel(smooth_wheel);
    crate::focus::set_boundary(focus_boundary);
}

/// The conventions the active surface was put on: the defaults until [`set_surface_conventions`] is called in it.
pub fn surface_conventions() -> SurfaceConventions {
    SurfaceConventions {
        grid: layout_reactive::layout_grid(),
        keyboard: platform_core::keyboard_convention(),
        smooth_wheel: ui_tree::smooth_wheel(),
        focus_boundary: crate::focus::boundary(),
    }
}

#[cfg(test)]
#[path = "conventions.test.rs"]
mod tests;
