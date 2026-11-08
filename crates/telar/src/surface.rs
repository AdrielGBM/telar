//! Handles on secondary surfaces opened at runtime, such as the desktop runner's `open_window`: what closes one.

/// A handle on a surface opened at runtime: what closes it.
pub trait SurfaceControl {
    fn close(&self);
    fn is_closing(&self) -> bool;
    /// Builds the surface's content again, in place — same window, same renderer, same position.
    ///
    /// Defaults to doing nothing, which is the honest answer for a backend that cannot remount a live surface: the caller's alternative is closing and reopening, and a host that silently did that would replace a surface the caller asked to keep.
    fn rebuild(&self) {}
}

/// What opening a surface at runtime hands back; dropping it closes the surface.
pub struct SurfaceToken {
    control: Box<dyn SurfaceControl>,
}

impl SurfaceToken {
    pub fn new(control: Box<dyn SurfaceControl>) -> Self {
        Self { control }
    }

    pub fn close(&self) {
        self.control.close();
    }

    pub fn is_closing(&self) -> bool {
        self.control.is_closing()
    }

    /// Asks the surface to build its content again without being replaced — how a surface follows something that changed underneath it (a config file, a theme) while keeping its place, its size and whatever state its content chose to keep outside the tree.
    pub fn rebuild(&self) {
        self.control.rebuild();
    }
}

impl Drop for SurfaceToken {
    fn drop(&mut self) {
        self.control.close();
    }
}
