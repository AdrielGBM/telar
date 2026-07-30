//! Backend-agnostic secondary-surface runtime: routes `open_surface` requests to an installed host; placement/scaffold widgets live in `ui-core`.

use std::cell::RefCell;

use ui_core::{LayoutItem, SurfacePlacement};

pub type SurfaceContent = Box<dyn FnOnce() -> Box<dyn LayoutItem> + Send>;

pub trait SurfaceControl {
    fn close(&self);
    fn is_closing(&self) -> bool;
}

pub trait SurfaceHost {
    fn open(&self, placement: SurfacePlacement, content: SurfaceContent) -> SurfaceToken;
}

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

thread_local! {
    static SURFACE_HOST: RefCell<Option<Box<dyn SurfaceHost>>> = const { RefCell::new(None) };
}

pub fn set_surface_host(host: Box<dyn SurfaceHost>) {
    SURFACE_HOST.with(|h| *h.borrow_mut() = Some(host));
}

pub fn has_surface_host() -> bool {
    SURFACE_HOST.with(|h| h.borrow().is_some())
}

pub fn open_surface(placement: SurfacePlacement, content: SurfaceContent) -> SurfaceToken {
    SURFACE_HOST.with(|h| match h.borrow().as_ref() {
        Some(host) => host.open(placement, content),
        None => {
            tracing::warn!(
                "telar::open_surface: no SurfaceHost installed on this thread; surface ignored"
            );
            SurfaceToken::new(Box::new(NoopControl))
        }
    })
}

struct NoopControl;

impl SurfaceControl for NoopControl {
    fn close(&self) {}
    fn is_closing(&self) -> bool {
        true
    }
}
