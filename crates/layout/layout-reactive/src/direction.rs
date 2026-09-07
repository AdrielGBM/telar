//! The active writing-direction signal — the reactive source that drives live LTR/RTL switching.
//!
//! Mirrors `theme-core`'s active-mode store and `i18n-core`'s active-locale store: a thread-local `RwSignal`, a setter, and a reactive getter. Unlike those two, the value is not read by the widgets themselves — [`compute_layout`](crate::compute_layout) reconciles each surface's layout engine with it before laying out, so a flip re-resolves the existing nodes rather than rebuilding any part of the tree. That is also what makes it reach every surface on the thread, not just whichever one was active at the call.

use layout_core::Direction;
use reactive_core::{RwSignal, detached, signal};

thread_local! {
    static DIRECTION: RwSignal<Direction> =
        detached(|| signal(Direction::Ltr));
}

/// Sets the writing direction every surface lays out against, taking effect on the next layout pass.
pub fn set_direction(direction: Direction) {
    DIRECTION.with(|s| {
        if s.peek() != direction {
            s.set(direction);
        }
    });
}

/// Reactive read of the active direction — subscribes the caller, for the rare widget that has to mirror something layout cannot flip on its own (a chevron glyph, a directional icon).
pub fn use_direction() -> Direction {
    DIRECTION.with(|s| s.get())
}

/// Non-reactive read of the active direction, for the layout pass and event handlers.
pub fn current_direction() -> Direction {
    DIRECTION.with(|s| s.peek())
}

#[cfg(test)]
#[path = "direction_test.rs"]
mod tests;
