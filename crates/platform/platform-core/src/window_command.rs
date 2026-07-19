use std::cell::{Cell, RefCell};

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
    /// Bring the window to the front and give it input focus. Honored where the window manager allows
    /// programmatic activation (some Wayland compositors ignore it).
    Focus,
}

thread_local! {
    // The live per-surface queue sits behind a swappable pointer (the same idiom as the reactive runtime's
    // cell); the cell holds a raw pointer and has no Drop, so no TLS destructor runs on thread exit. This
    // crate has no reactive-core dependency, so the swap is hand-written here rather than via `surface_local!`.
    static WINDOW_COMMANDS: Cell<*mut RefCell<Vec<WindowCommand>>> =
        Cell::new(Box::into_raw(Box::new(RefCell::new(Vec::new()))));
}

fn with_commands<R>(f: impl FnOnce(&mut Vec<WindowCommand>) -> R) -> R {
    // SAFETY: the pointer always addresses a live `RefCell<Vec<WindowCommand>>` (the leaked ambient queue or
    // a `WindowCommandContext` box that outlives every guard pointing the cell at it); the borrow is released
    // before the closure returns.
    WINDOW_COMMANDS.with(|cell| unsafe { f(&mut *(*cell.get()).borrow_mut()) })
}

/// Enqueue a window-management command from UI code (e.g. a title-bar button's `on_press`). The runner drains
/// the queue after event dispatch and applies each command to the OS window. Lives in a thread-local so it
/// works from any widget closure without threading a window handle through the tree; each surface owns its
/// own queue (activated via [`WindowCommandContext`]), so commands never cross windows.
pub fn push_window_command(command: WindowCommand) {
    with_commands(|q| q.push(command));
}

/// Drain every queued window command. Called by the runner once per event-dispatch cycle.
pub fn take_window_commands() -> Vec<WindowCommand> {
    with_commands(std::mem::take)
}

/// A per-surface window-command queue. The runner activates each surface's context around its event dispatch
/// so a title-bar action targets that surface's window; the guard restores the previous surface's queue.
pub struct WindowCommandContext {
    ptr: *mut RefCell<Vec<WindowCommand>>,
}

impl WindowCommandContext {
    pub fn new() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(RefCell::new(Vec::new()))),
        }
    }

    #[must_use = "the surface context is only active while this guard is alive"]
    pub fn enter(&self) -> WindowCommandGuard {
        let prev = WINDOW_COMMANDS.with(|cell| cell.replace(self.ptr));
        WindowCommandGuard { prev }
    }
}

impl Default for WindowCommandContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WindowCommandContext {
    fn drop(&mut self) {
        // A matching guard has restored the previous queue, so this box is not the live pointer.
        unsafe { drop(Box::from_raw(self.ptr)) };
    }
}

#[must_use = "the surface context is only active while this guard is alive"]
pub struct WindowCommandGuard {
    prev: *mut RefCell<Vec<WindowCommand>>,
}

impl Drop for WindowCommandGuard {
    fn drop(&mut self) {
        WINDOW_COMMANDS.with(|cell| cell.set(self.prev));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The M3 hazard: many surfaces share one UI thread, so a `Close` pushed by one window's title bar must
    // land only in that window's queue — otherwise a sibling drains it and the wrong window closes.
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
