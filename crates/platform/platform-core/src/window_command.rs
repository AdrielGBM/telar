use std::cell::RefCell;

/// A window-management action requested by UI code (a custom title bar's buttons) and applied by the runner
/// to the OS window after event dispatch. Enqueued via [`push_window_command`] and drained via
/// [`take_window_commands`]; kept as data (rather than direct `Window` calls) because widget closures run
/// deep in the tree walk with no access to the platform window.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommand {
    /// Begin an OS-driven interactive move (drag the window by its title bar). Must originate from a
    /// pointer-press handler so the platform can latch onto the in-progress drag.
    Drag,
    /// Minimize the window to the taskbar/dock.
    Minimize,
    /// Toggle between maximized and restored.
    ToggleMaximize,
    /// Explicitly set the maximized state.
    SetMaximized(bool),
    /// Close the window (and, for a single-window app, exit the app).
    Close,
    /// Update the OS window title.
    SetTitle(String),
}

thread_local! {
    static WINDOW_COMMANDS: RefCell<Vec<WindowCommand>> = const { RefCell::new(Vec::new()) };
}

/// Enqueue a window-management command from UI code (e.g. a title-bar button's `on_press`). The runner drains
/// the queue after event dispatch and applies each command to the OS window. Lives in a thread-local so it
/// works from any widget closure without threading a window handle through the tree; each surface's worker
/// thread owns its own queue (see the multi-surface backend), so commands never cross windows.
pub fn push_window_command(command: WindowCommand) {
    WINDOW_COMMANDS.with(|q| q.borrow_mut().push(command));
}

/// Drain every queued window command. Called by the runner once per event-dispatch cycle.
pub fn take_window_commands() -> Vec<WindowCommand> {
    WINDOW_COMMANDS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}
