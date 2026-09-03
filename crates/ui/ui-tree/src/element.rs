//! Whether a frame carries the structure of its boxes, and not only their pixels.
//!
//! Off by default. A rasteriser is handed rects that are already positioned and has no use for a `<button>`
//! being a button, so on a desktop build the extra pair of commands per widget would be paid for by nobody.
//! A backend whose output is a document turns it on before the first frame; so do the tests that check what
//! such a backend would emit, which is what lets that be checked on a machine with no browser on it.

use std::cell::Cell;

thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
}

/// Turns element capture on or off for this thread, returning what it was.
///
/// Per thread rather than per process because a surface is: two surfaces on one thread share it, and a test
/// that turns it on does not reach into another test's.
pub fn set_element_capture(on: bool) -> bool {
    CAPTURE.replace(on)
}

/// Whether widgets should wrap what they draw in an element. Read once per `view()`, so it is a thread-local
/// load on a path that already does far more than that.
#[inline]
pub fn element_capture() -> bool {
    CAPTURE.get()
}
