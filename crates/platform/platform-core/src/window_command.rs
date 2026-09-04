//! Window-management actions UI code enqueues and the runner applies.

/// A window-management action requested by UI code (a custom title bar's buttons) and applied by the runner to the OS window after event dispatch. Enqueued via [`push_window_command`] and drained via [`take_window_commands`]; kept as data (rather than direct `Window` calls) because widget closures run deep in the tree walk with no access to the platform window.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommand {
    /// Begin an OS-driven interactive move (drag the window by its title bar). Must originate from a pointer-press handler so the platform can latch onto the in-progress drag.
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
    /// Bring the window to the front and give it input focus. Honored where the window manager allows programmatic activation (some Wayland compositors ignore it).
    Focus,
    /// Set the pointer shape. Pushed from a hover handler, which is why it is a command rather than a property: the widget under the pointer decides, and it changes many times per second.
    SetCursor(crate::Cursor),
}

reactive_local::surface_local! {
    /// A per-surface window-command queue. The runner activates each surface's context around its event dispatch so a title-bar action targets that surface's window; the guard restores the previous one.
    slot WINDOW_COMMANDS: Vec<WindowCommand> = Vec::new();
    access with_commands, with_commands_ref;
    context WindowCommandContext, WindowCommandGuard;
}

/// Enqueue a window-management command from UI code (e.g. a title-bar button's `on_press`). The runner drains the queue after event dispatch and applies each command to the OS window. Lives in a thread-local so it works from any widget closure without threading a window handle through the tree; each surface owns its own queue (activated via [`WindowCommandContext`]), so commands never cross windows.
pub fn push_window_command(command: WindowCommand) {
    with_commands(|q| q.push(command));
}

/// Drain every queued window command. Called by the runner once per event-dispatch cycle.
pub fn take_window_commands() -> Vec<WindowCommand> {
    with_commands(std::mem::take)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The M3 hazard: many surfaces share one UI thread, so a `Close` pushed by one window's title bar must land only in that window's queue — otherwise a sibling drains it and the wrong window closes.
    #[test]
    fn command_pushed_under_one_context_does_not_leak_to_another() {
        let root = WindowCommandContext::new();
        let child = WindowCommandContext::new();

        {
            let _root = root.enter();
            push_window_command(WindowCommand::Close);
        }
        {
            let _child = child.enter();
            assert!(take_window_commands().is_empty());
        }
        {
            let _root = root.enter();
            assert_eq!(take_window_commands(), vec![WindowCommand::Close]);
        }
    }

    // A guard restores the queue that was active before `enter`, so nested contexts unwind in order.
    #[test]
    fn dropping_a_guard_restores_the_previous_surfaces_queue() {
        let outer = WindowCommandContext::new();
        let inner = WindowCommandContext::new();

        let _outer = outer.enter();
        push_window_command(WindowCommand::Minimize);
        {
            let _inner = inner.enter();
            push_window_command(WindowCommand::Close);
            assert_eq!(take_window_commands(), vec![WindowCommand::Close]);
        }
        assert_eq!(take_window_commands(), vec![WindowCommand::Minimize]);
    }
}
