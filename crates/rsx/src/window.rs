//! Window-management calls for UI code. A custom title bar's buttons call these directly — including from
//! `.rsx` `on_press` handlers (`on_press(|| rsx::window::close())`). Each enqueues a
//! [`platform_core::WindowCommand`] that the runner applies to the OS window right after the current event is
//! dispatched. On backends without a movable top-level window (layer-shell, headless) they are inert no-ops.

use platform_core::{WindowCommand, push_window_command};

/// Begin an OS-driven interactive move. Call from a title-bar **pointer-press** handler so the platform can
/// latch onto the drag while the button is held.
pub fn drag() {
    push_window_command(WindowCommand::Drag);
}

/// Minimize the window to the taskbar/dock.
pub fn minimize() {
    push_window_command(WindowCommand::Minimize);
}

/// Toggle between maximized and restored.
pub fn toggle_maximize() {
    push_window_command(WindowCommand::ToggleMaximize);
}

/// Explicitly set the maximized state.
pub fn set_maximized(maximized: bool) {
    push_window_command(WindowCommand::SetMaximized(maximized));
}

/// Close the window (and, for a single-window app, exit the app).
pub fn close() {
    push_window_command(WindowCommand::Close);
}

/// Update the OS window title.
pub fn set_title(title: impl Into<String>) {
    push_window_command(WindowCommand::SetTitle(title.into()));
}
