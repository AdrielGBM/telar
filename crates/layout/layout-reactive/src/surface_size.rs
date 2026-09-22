//! How big each surface is, as reactive state: written by whatever hosts the tree when its surface is resized, read by the layout pass (for lengths written as a fraction of the surface) and by anything that chooses by size.
//!
//! Per surface, because every window, page and terminal has its own, and a component reads the one it was built on. One signal per side, so what follows only the width is not woken by a height that moves on its own — a phone's toolbar sliding away does exactly that.

use geometry_core::Size;
use reactive_core::{RwSignal, batch, signal};

#[derive(Clone, Copy)]
struct Sides {
    width: RwSignal<f32>,
    height: RwSignal<f32>,
}

reactive_core::surface_local! {
    /// The active surface's size, in the logical units its layout is written in.
    slot SIDES: Sides = Sides { width: signal(0.0), height: signal(0.0) };
    access with_sides, with_sides_ref;
    context SurfaceSizeContext, SurfaceSizeGuard;
}

// Copied out of the slot before any write: a write flushes effects, and one of them reading the size would re-enter the slot's borrow.
fn sides() -> Sides {
    with_sides_ref(|sides| *sides)
}

/// Records the active surface's new size, notifying only the sides that moved. The runner calls this before the tree hears the resize, so the layout pass that answers it already resolves against the new size.
pub fn set_surface_size(size: Size) {
    let sides = sides();
    batch(|| {
        if sides.width.peek() != size.width {
            sides.width.set(size.width);
        }
        if sides.height.peek() != size.height {
            sides.height.set(size.height);
        }
    });
}

/// Non-reactive read of the active surface's size, for the layout pass and event handlers.
pub fn surface_size() -> Size {
    let sides = sides();
    Size::new(sides.width.peek(), sides.height.peek())
}

/// Reactive read of the active surface's size; subscribes the caller to both sides.
pub fn use_surface_size() -> Size {
    let sides = sides();
    Size::new(sides.width.get(), sides.height.get())
}

/// Reactive read of the active surface's width alone.
pub fn use_surface_width() -> f32 {
    sides().width.get()
}

/// Reactive read of the active surface's height alone.
pub fn use_surface_height() -> f32 {
    sides().height.get()
}

#[cfg(test)]
#[path = "surface_size_test.rs"]
mod tests;
