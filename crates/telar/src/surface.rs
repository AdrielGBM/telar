//! Handles on secondary surfaces opened at runtime, such as the desktop runner's `open_window`: what closes one.

/// A handle on a surface opened at runtime: what closes it.
pub trait SurfaceControl {
    fn close(&self);
    fn is_closing(&self) -> bool;
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
}

impl Drop for SurfaceToken {
    fn drop(&mut self) {
        self.control.close();
    }
}
