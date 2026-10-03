//! Window-management calls for UI code. A custom title bar's buttons call these directly — including from `.rsx` `on_press` handlers (`on_press(|| telar::window::close())`). Each enqueues a [`platform_core::WindowCommand`] that the runner applies to the OS window right after the current event is dispatched. On backends without a movable top-level window (layer-shell, headless) they are inert no-ops.

use platform_core::{WindowCommand, push_window_command};

/// Begin an OS-driven interactive move. Call from a title-bar **pointer-press** handler so the platform can latch onto the drag while the button is held.
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

/// Renames the app in this window's title. The title the window shows is derived from it and from the page the app's address stands on, `Credits — Portfolio`, and follows both; see [`set_title_format`](crate::set_title_format) for another rule and `docs/surface-title.md` for what each target shows it in.
pub fn set_title(title: impl Into<String>) {
    ui_core::set_app_title(title);
}

/// Reactive read of the title this window shows, for a custom title bar to draw.
pub fn title() -> String {
    ui_core::use_surface_title()
}

/// Bring this window to the front and give it input focus. Applied by the runner after the current event or frame; some Wayland compositors forbid programmatic activation, where it is a no-op.
pub fn focus() {
    push_window_command(WindowCommand::Focus);
}
