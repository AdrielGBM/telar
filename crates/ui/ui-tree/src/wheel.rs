//! Whether a wheel notch is worth animating on the active surface.
//!
//! On by default, because a surface that draws in pixels can show the intermediate positions and every desktop platform does: a notch that lands in one frame gives no sense of which way the content went.
//!
//! A terminal turns it off. Its smallest visible step is a whole cell, so easing across a notch would repaint the screen several times to show the same two or three rows — a stutter drawn at the cost of a glide.

reactive_core::surface_local! {
    /// Whether one surface eases a wheel notch.
    slot SMOOTH: bool = true;
    access with_smooth, with_smooth_ref;
    context WheelContext;
}

/// Turns wheel smoothing on or off for the active surface, as whatever hosts it puts the surface on its platform's conventions.
pub fn set_smooth_wheel(on: bool) {
    with_smooth(|smooth| *smooth = on);
}

/// Whether a wheel notch on the active surface should be eased rather than applied whole.
pub fn smooth_wheel() -> bool {
    with_smooth_ref(|smooth| *smooth)
}
