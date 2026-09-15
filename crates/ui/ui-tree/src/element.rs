//!
//! Off by default: a rasteriser reads none of this, so building it would cost every desktop frame for a reader that doesn't exist; a document backend turns it on before its first frame (and so do the tests that emulate one without a browser).

use std::cell::Cell;

thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
}

/// Turns element capture on or off for this thread, returning what it was.
///
/// Per thread rather than per process because a surface is: two surfaces on one thread share it, and a test that turns it on does not reach into another test's.
pub fn set_element_capture(on: bool) -> bool {
    CAPTURE.replace(on)
}

/// Read once per `view()`, so it is a thread-local load on a path that already does far more than that.
#[inline]
pub fn element_capture() -> bool {
    CAPTURE.get()
}
