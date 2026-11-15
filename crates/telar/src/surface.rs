//! Handles on secondary surfaces opened at runtime, such as the desktop runner's `open_window`: what closes one.

/// A handle on a surface opened at runtime: what closes it.
pub trait SurfaceControl {
    /// Closes the surface.
    fn close(&self);
    /// Whether the surface has been asked to close or has closed.
    fn is_closing(&self) -> bool;
}

/// What opening a surface at runtime hands back; dropping it closes the surface.
pub struct SurfaceToken {
    control: Box<dyn SurfaceControl>,
}

impl SurfaceToken {
    /// A token that closes the surface `control` stands for when dropped.
    pub fn new(control: Box<dyn SurfaceControl>) -> Self {
        Self { control }
    }

    /// Closes the surface now rather than when the token drops.
    pub fn close(&self) {
        self.control.close();
    }

    /// Whether the surface is closing or closed.
    pub fn is_closing(&self) -> bool {
        self.control.is_closing()
    }
}

impl Drop for SurfaceToken {
    fn drop(&mut self) {
        self.control.close();
    }
}
