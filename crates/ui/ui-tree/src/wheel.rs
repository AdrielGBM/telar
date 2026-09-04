//! Whether a wheel notch is worth animating on this surface.
//!
//! On by default, because a surface that draws in pixels can show the intermediate positions and every desktop platform does: a notch that lands in one frame gives no sense of which way the content went.
//!
//! A terminal turns it off. Its smallest visible step is a whole cell, so easing across a notch would repaint the screen several times to show the same two or three rows — a stutter drawn at the cost of a glide.

use std::cell::Cell;

thread_local! {
    static SMOOTH: Cell<bool> = const { Cell::new(true) };
}

/// Turns wheel smoothing on or off for this thread, returning what it was.
pub fn set_smooth_wheel(on: bool) -> bool {
    SMOOTH.replace(on)
}

/// Whether a wheel notch should be eased rather than applied whole.
#[inline]
pub fn smooth_wheel() -> bool {
    SMOOTH.get()
}
