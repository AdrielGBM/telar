//! Where the keyboard goes when Tab runs off either end of an application's focus order.

/// What a platform does with the keyboard when Tab steps past the last focusable, or Shift+Tab past the first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusBoundary {
    /// Focus carries on at the other end: the application's order is a loop, as a window's is.
    #[default]
    Wrap,
    /// Focus leaves the application, for whatever the platform has around it: a page's other controls, the browser's own interface.
    Release,
}

/// Which of Tab and Shift+Tab, pressed now, would run off an end of the focus order and so leave the application, for a platform that has to decide whether a key press is the application's before the application has heard of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabLeaves {
    pub forward: bool,
    pub backward: bool,
}
