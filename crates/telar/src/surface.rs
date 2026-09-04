//! Backend-agnostic secondary-surface runtime: routes `open_surface` requests to an installed host.
//!
//! The placement type is the backend's own. Telar carried a `SurfacePlacement` vocabulary of its own for a while, and it bought nothing: the producer and the implementor were two crates of the *same* application, so both ends paid a translation hop into and out of a framework type whose majority of fields the framework never read. What is framework-shaped here is the indirection — a thread-local host, keyed by the placement type — not the description of where a panel sits.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

use std::collections::HashMap;
use ui_core::LayoutItem;

/// How a hosted surface's content is built — and rebuilt.
///
/// `Fn` rather than `FnOnce` because a surface can outlive the state it was built from: a backend that can remount a live surface (see [`SurfaceControl::rebuild`]) calls this again on the same surface, so the closure has to be able to produce the tree more than once. In practice that means capturing what the content is *about* — an id, an edge — and resolving the rest inside, which is also what makes the rebuild worth doing.
pub type SurfaceContent = Rc<dyn Fn() -> Box<dyn LayoutItem>>;

/// Turns a closure into a [`SurfaceContent`].
pub fn surface_content(build: impl Fn() -> Box<dyn LayoutItem> + 'static) -> SurfaceContent {
    Rc::new(build)
}

/// A handle on a surface opened at runtime: what closes it.
pub trait SurfaceControl {
    fn close(&self);
    fn is_closing(&self) -> bool;
    /// Builds the surface's content again, in place — same window, same renderer, same position.
    ///
    /// Defaults to doing nothing, which is the honest answer for a backend that cannot remount a live surface: the caller's alternative is closing and reopening, and a host that silently did that would replace a surface the caller asked to keep.
    fn rebuild(&self) {}
}

/// A backend registers a host for *its own* placement type `P`, and callers reach it by naming that same type.
pub trait SurfaceHost<P: 'static>: 'static {
    fn open(&self, placement: P, content: SurfaceContent) -> SurfaceToken;
}

/// What `open_surface` hands back; dropping it does not close the surface.
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

thread_local! {
    // Keyed by the placement type because the trait is generic and so not object-safe on its own. The stored value is a `Box<dyn SurfaceHost<P>>` erased once more into `dyn Any`, which is a concrete `'static` type per `P` and so downcasts back exactly.
    static SURFACE_HOSTS: RefCell<HashMap<TypeId, Box<dyn Any>>> =
        RefCell::new(HashMap::new());
}

/// Installs `host` as the backend for placements of type `P`. A later call for the same `P` replaces it.
pub fn set_surface_host<P: 'static>(host: impl SurfaceHost<P>) {
    let host: Box<dyn SurfaceHost<P>> = Box::new(host);
    SURFACE_HOSTS.with(|hosts| {
        hosts
            .borrow_mut()
            .insert(TypeId::of::<P>(), Box::new(host) as Box<dyn Any>)
    });
}

/// Opens another surface on the running loop, created by the runner on its next turn.
pub fn open_surface<P: 'static>(placement: P, content: SurfaceContent) -> SurfaceToken {
    SURFACE_HOSTS.with(|hosts| {
        let hosts = hosts.borrow();
        match hosts
            .get(&TypeId::of::<P>())
            .and_then(|host| host.downcast_ref::<Box<dyn SurfaceHost<P>>>())
        {
            Some(host) => host.open(placement, content),
            None => {
                tracing::warn!(
                    "telar::open_surface: no SurfaceHost installed on this thread for {}; surface ignored",
                    std::any::type_name::<P>()
                );
                SurfaceToken::new(Box::new(NoopControl))
            }
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
