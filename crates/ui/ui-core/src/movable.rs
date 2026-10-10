//! [`Movable`]: a subtree built once and shown wherever its newest live outlet stands, moved there rather than built again.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{RwSignal, batch, on_cleanup, signal, with_owner};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{new_container, set_children};
use crate::disposal::retire;
use crate::focus;
use crate::layout_item::{BuildPass, Child, LayoutItem};
use crate::pointer::dispatch_container_event;
use crate::surface_context::HomeSurface;

/// A subtree built once and shown in whichever of its outlets is live, moved from one to the next instead of being rebuilt.
///
/// What a layout that rearranges itself needs: a pane that sits beside the sidebar in a wide window and under a drawer in a narrow one sits under two different parents, and rebuilding it for each loses everything it held — scroll, focus, a running animation, a mounted surface and whatever runs on it.
///
/// [`new`](Self::new) builds the subtree under an owner of its own, and [`outlet`](Self::outlet) makes the box it is shown in. The newest outlet still alive shows it; when that outlet goes, the one before it takes the subtree back, and with none left the subtree is *parked*: kept, but not laid out, drawn, offered the pointer or reached by Tab, and focus inside it is let go. A layer or an overlay open inside it is hidden with it, as under any hidden box, rather than closed, and is back when an outlet shows the subtree again. An arrangement rebuilt around the subtree builds its new outlet before the old one is disposed, so the subtree goes straight from one to the other.
///
/// What the subtree reads from its surroundings comes from two places, as it does for a portal in a document:
/// - what an owner provides — the theme tokens, the locale, a [`provide_context`](reactive_core::provide_context) value — is read from where `new` was called, for the subtree's whole life, wherever it is shown;
/// - what the layout tree carries — text styles declared on the boxes above it, a theme a [`ThemeProvider`](crate::ThemeProvider) put on one of them, being hidden, disabled, clipped or scrolled — follows the outlet showing it, and is resolved again when it moves.
///
/// It is shown only on the surface it was built on: an outlet asked for on another one is an error. An outlet must not stand inside the subtree it shows.
///
/// A cheap handle: clones share the one subtree, which is freed when the last clone drops or the scope `new` was called in is disposed, whichever comes first. An outlet does not keep it alive.
#[derive(Clone)]
pub struct Movable(Rc<Shared>);

struct Shared {
    home: HomeSurface,
    content: RefCell<Option<Child>>,
    /// The live outlets in the order they were made: the last one shows the subtree.
    outlets: RefCell<Vec<Outlet>>,
    shown: Cell<bool>,
    /// A hidden box the subtree hangs from while it is parked, so it is hidden there as under any hidden box: not laid out, drawn, offered the pointer or reached by Tab, and neither are the layers and overlays opened inside it. Freed with the subtree.
    parking: NodeId,
    next_outlet: Cell<u64>,
}

/// An outlet as the subtree knows it. Its signal belongs to the outlet's own scope, which is alive for as long as the outlet is in [`Shared::outlets`]: the scope's cleanup is what takes it out.
#[derive(Clone, Copy)]
struct Outlet {
    id: u64,
    node: NodeId,
    hosting: RwSignal<bool>,
}

impl Movable {
    /// Builds the subtree now, under an owner of its own beneath the current one, and parks it until an outlet shows it.
    pub fn new(
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, LayoutError> {
        let mut pass = BuildPass::start();
        let content = pass.build_child(build)?;
        pass.keep();
        let node = content.node();
        let owner = content.owner();
        let parking = match new_container(LayoutStyle::new().shown(false), &[node]) {
            Ok(parking) => parking,
            Err(err) => {
                retire(owner, node);
                return Err(err);
            }
        };
        layout_reactive::free_with(parking, node);
        let shared = Rc::new(Shared {
            home: HomeSurface::here(),
            content: RefCell::new(Some(content)),
            outlets: RefCell::new(Vec::new()),
            shown: Cell::new(false),
            parking,
            next_outlet: Cell::new(0),
        });
        let disposing = Rc::downgrade(&shared);
        with_owner(owner, || {
            on_cleanup(move || {
                if let Some(shared) = disposing.upgrade() {
                    shared.dispose();
                }
            })
        });
        Ok(Self(shared))
    }

    /// A box laid out with `style` that shows the subtree from now on, taking it from whichever outlet showed it before. Dropped or disposed, it hands the subtree back to the newest outlet still alive.
    pub fn outlet(&self, style: LayoutStyle) -> Result<MovableOutlet, LayoutError> {
        if HomeSurface::here() != self.0.home {
            return Err(LayoutError::Engine(
                "a movable subtree is shown only on the surface it was built on".into(),
            ));
        }
        let node = new_container(style, &[])?;
        let hosting = signal(false);
        let id = self.0.next_outlet.get();
        self.0.next_outlet.set(id + 1);
        let movable = Rc::downgrade(&self.0);
        let releasing = movable.clone();
        on_cleanup(move || release(&releasing, id));
        self.0.claim(Outlet { id, node, hosting });
        Ok(MovableOutlet {
            node,
            id,
            hosting,
            movable,
        })
    }

    /// Whether an outlet shows the subtree right now, rather than it being parked.
    pub fn is_shown(&self) -> bool {
        self.0.shown.get()
    }

    /// The subtree's root layout node, until it is freed.
    pub fn layout_node(&self) -> Option<NodeId> {
        self.0.content_node()
    }
}

impl Shared {
    fn content_node(&self) -> Option<NodeId> {
        self.content.borrow().as_ref().map(Child::node)
    }

    fn host(&self) -> Option<Outlet> {
        self.outlets.borrow().last().copied()
    }

    fn claim(&self, outlet: Outlet) {
        let previous = self.host();
        self.outlets.borrow_mut().push(outlet);
        batch(|| {
            if let Some(previous) = previous {
                previous.hosting.set(false);
            }
            self.show_in(outlet);
        });
    }

    fn show_in(&self, outlet: Outlet) {
        let Some(node) = self.content_node() else {
            return;
        };
        // Takes the node from the outlet that held it, which taffy does as part of hanging it here.
        let _ = set_children(outlet.node, &[node]);
        outlet.hosting.set(true);
        self.shown.set(true);
        moved();
    }

    fn park(&self, node: NodeId) {
        let _ = set_children(self.parking, &[node]);
        self.shown.set(false);
        moved();
    }

    fn release(&self, id: u64) {
        let (released, was_showing, next) = {
            let mut outlets = self.outlets.borrow_mut();
            let Some(at) = outlets.iter().position(|outlet| outlet.id == id) else {
                return;
            };
            let released = outlets.remove(at);
            (released, at == outlets.len(), outlets.last().copied())
        };
        let Some(node) = self.content_node().filter(|_| was_showing) else {
            return;
        };
        let Some(_entered) = self.home.enter() else {
            return;
        };
        batch(|| {
            released.hosting.set(false);
            match next {
                Some(next) => self.show_in(next),
                None => self.park(node),
            }
        });
    }

    /// Frees the subtree, owner before node, unless it is already gone. The outlets stay, empty.
    fn dispose(&self) {
        let Some(content) = self.content.borrow_mut().take() else {
            return;
        };
        let host = self.host();
        self.outlets.borrow_mut().clear();
        self.shown.set(false);
        let Some(_entered) = self.home.enter() else {
            return;
        };
        if let Some(host) = host {
            host.hosting.set(false);
        }
        retire(content.owner(), content.node());
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn release(movable: &Weak<Shared>, id: u64) {
    if let Some(shared) = movable.upgrade() {
        shared.release(id);
    }
}

/// A subtree hung from a new parent inherits from new ancestors and may have moved under a hidden or disabled one, and no declaration changed to say so.
fn moved() {
    crate::inherit::restructured();
    focus::reach_changed();
}

/// The box a [`Movable`] is shown in, from [`Movable::outlet`]: a plain box holding the subtree while it is the newest live outlet, and empty otherwise.
pub struct MovableOutlet {
    node: NodeId,
    id: u64,
    hosting: RwSignal<bool>,
    movable: Weak<Shared>,
}

impl MovableOutlet {
    /// Whether this outlet shows the subtree right now. Reactive.
    pub fn is_showing(&self) -> bool {
        self.hosting.get()
    }
}

impl LayoutItem for MovableOutlet {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for MovableOutlet {
    fn view(&self) -> RenderNode {
        let content = self
            .is_showing()
            .then(|| self.movable.upgrade())
            .flatten()
            .and_then(|shared| shared.content.borrow().as_ref().map(Child::boundary))
            .unwrap_or(RenderNode::Empty);
        crate::element::wrap(self.node, content)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if !self.hosting.peek() {
            return EventResult::Ignored;
        }
        // Held for the whole dispatch, so a handler inside that drops the last handle frees the subtree once it has returned.
        let Some(shared) = self.movable.upgrade() else {
            return EventResult::Ignored;
        };
        let Some(content) = shared.content.borrow().clone() else {
            return EventResult::Ignored;
        };
        dispatch_container_event(&mut vec![content], event)
    }

    fn debug_name(&self) -> &'static str {
        "MovableOutlet"
    }
}

impl Drop for MovableOutlet {
    fn drop(&mut self) {
        release(&self.movable, self.id);
    }
}

#[cfg(test)]
#[path = "movable_test.rs"]
mod tests;
