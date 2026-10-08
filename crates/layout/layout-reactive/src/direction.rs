//! The writing direction: a thread-wide value every surface lays out against, and a per-surface override that takes its place on the surface that sets one.
//!
//! The value is not read by the widgets themselves — [`compute_layout`](crate::compute_layout) reconciles each surface's layout engine with it before laying out, so a flip re-resolves the existing nodes rather than rebuilding any part of the tree. The override is per surface rather than per subtree for the same reason: one layout engine resolves one direction for its whole tree.

use layout_core::Direction;

reactive_core::surface_scoped! {
    /// The active surface's own writing direction, in place of the thread's while it has one.
    scoped direction: Direction = Direction::Ltr;
    context DirectionContext, DirectionGuard;
}

/// Sets the writing direction every surface without an override of its own lays out against, taking effect on the next layout pass.
pub fn set_direction(direction: Direction) {
    self::direction().set(direction);
}

/// Gives the active surface a writing direction of its own, or with `None` hands it back to the thread's [`set_direction`]. Takes effect on the surface's next layout pass.
pub fn set_surface_direction(direction: Option<Direction>) {
    self::direction().set_surface(direction.as_ref());
}

/// Reactive read of the active surface's own direction, `None` where it follows the thread's.
pub fn use_surface_direction() -> Option<Direction> {
    direction().surface()
}

/// Reactive read of the direction in force on the active surface — subscribes the caller, for the rare widget that has to mirror something layout cannot flip on its own (a chevron glyph, a directional icon).
pub fn use_direction() -> Direction {
    direction().get()
}

/// Non-reactive read of the direction in force on the active surface, for the layout pass and event handlers.
pub fn current_direction() -> Direction {
    direction().peek()
}

#[cfg(test)]
#[path = "direction_test.rs"]
mod tests;
