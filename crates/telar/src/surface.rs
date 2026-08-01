//! Backend-agnostic secondary-surface runtime: routes `open_surface` requests to an installed host; placement/scaffold widgets live in `ui-core`.

use std::cell::RefCell;
use std::rc::Rc;

use ui_core::{LayoutItem, SurfacePlacement};

/// How a hosted surface's content is built — and rebuilt.
///
/// `Fn` rather than `FnOnce` because a surface can outlive the state it was built from: a backend that can
/// remount a live surface (see [`SurfaceControl::rebuild`]) calls this again on the same surface, so the
/// closure has to be able to produce the tree more than once. In practice that means capturing what the
/// content is *about* — an id, an edge — and resolving the rest inside, which is also what makes the rebuild
/// worth doing.
pub type SurfaceContent = Rc<dyn Fn() -> Box<dyn LayoutItem>>;

/// Turns a closure into a [`SurfaceContent`].
pub fn surface_content(build: impl Fn() -> Box<dyn LayoutItem> + 'static) -> SurfaceContent {
    Rc::new(build)
}

pub trait SurfaceControl {
    fn close(&self);
    fn is_closing(&self) -> bool;
    /// Builds the surface's content again, in place — same window, same renderer, same position.
    ///
    /// Defaults to doing nothing, which is the honest answer for a backend that cannot remount a live surface:
    /// the caller's alternative is closing and reopening, and a host that silently did that would replace a
    /// surface the caller asked to keep.
    fn rebuild(&self) {}
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

    /// Asks the surface to build its content again without being replaced — how a surface follows something
    /// that changed underneath it (a config file, a theme) while keeping its place, its size and whatever
    /// state its content chose to keep outside the tree.
    pub fn rebuild(&self) {
        self.control.rebuild();
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
