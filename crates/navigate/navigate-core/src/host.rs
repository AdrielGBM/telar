//! [`NavHost`]: the widget that renders whichever page a [`Navigator`](crate::Navigator) names, and keeps the pages behind it alive.

use std::cell::RefCell;
use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, SizeDimension};
use motion_core::Animated;
use platform_core::Event;
use reactive_core::{OwnerId, dispose_owner, owner_scope, with_owner};
use ui_core::{
    Component, EventResult, LayoutItem, NodeId, RenderNode, absolute_rect, mark_dirty,
    new_container, remove_node, set_children, set_display,
};

use crate::navigator::Navigator;
use crate::page::{NavPage, PagePolicy};
use crate::transition::NavTransition;

/// Builds a page for a route on its first visit — returns any [`NavPage`], or a [`LayoutError`] if the page's widgets fail to construct.
type PageFactory<R> = dyn Fn(&R) -> Result<Box<dyn NavPage>, LayoutError>;

/// What a built page is filed under — the runtime form of [`PagePolicy`].
///
/// A stack only ever mutates at its top (`push`/`pop`/`replace`/`reset` truncate or touch the last entry), so an entry never changes position while it lives: its index is a stable identity, with no per-entry id to mint or to carry through a hot-reload snapshot. The route is part of the key so `replace`, which reuses the top index for a different destination, still rebuilds.
#[derive(Clone, PartialEq)]
enum PageKey<R> {
    /// [`PagePolicy::KeepAlive`]: one page per route, shared by every stack position naming it.
    Route(R),
    /// [`PagePolicy::Transient`]: one page per stack entry.
    Entry { slot: usize, route: R },
}

impl<R: Clone + Eq> PageKey<R> {
    fn route(&self) -> &R {
        match self {
            PageKey::Route(r) | PageKey::Entry { route: r, .. } => r,
        }
    }

    /// Whether the key still names a live stack entry. A route-keyed page always does — it belongs to the host, not to a position — while an entry-keyed one dies with its slot (popped past, replaced, or reset away).
    fn is_live(&self, stack: &[R]) -> bool {
        match self {
            PageKey::Route(_) => true,
            PageKey::Entry { slot, route } => stack.get(*slot) == Some(route),
        }
    }
}

struct BuiltPage<R> {
    key: PageKey<R>,
    /// Its own cell, so a page is dispatched to while the host's own state is not borrowed — see [`Seen`].
    page: Rc<RefCell<Box<dyn NavPage>>>,
    node: NodeId,
    /// Everything the page's build created. A pruned page frees its layout node, so whatever it registered against that node has to go first — and its effects have to stop before they fire at a node that is gone.
    owner: OwnerId,
}

/// A running entrance animation for the page that just became active, plus its direction (forward push vs. back pop) so a slide enters from the correct side.
struct Entrance<R> {
    key: PageKey<R>,
    forward: bool,
    anim: Animated<f32>,
}

/// Everything a reconcile moves, behind one cell.
///
/// **A navigation is state, and state settles when the runtime flushes** — not when an event happens to walk past. The two are the same moment often enough to be mistaken for one, and were: the host reconciled in [`on_event`](Component::on_event) and nowhere else. What broke it is a press on an overlay. The loop offers a positioned event to the overlays first and, when one takes it, **skips the tree walk entirely** — that is the block that keeps a press on a panel off the pane behind it. A route changed from inside such a handler moved the navigator and reached no `on_event`, so the window went on drawing the page it was on, at full rate, until some later event did reach the tree: a menu row that navigates, and a screen that changes when the pointer next moves.
///
/// So the reconcile also happens where the subscription already was — [`view`](Component::view) reads the navigator, and now settles against it before it renders. `on_event` still settles first where it can, which is what keeps a press that *is* dispatched through the tree landing on the frame it was made in.
///
/// What that costs is interior mutability, and the borrows are why a page is its own cell: dispatching to one must not hold this open, or a handler that navigates re-enters it from the flush that follows.
struct Seen<R> {
    pages: Vec<BuiltPage<R>>,
    /// The route currently displayed — the shadow reconciled against `nav.current()`.
    current: R,
    /// Key of the displayed page: which of two pages for the same route is on screen, when the policy makes them distinct.
    current_key: PageKey<R>,
    /// Stack depth at the last applied navigation, to tell a forward push from a back pop.
    prev_depth: usize,
    entrance: Option<Entrance<R>>,
}

/// A container that renders the top of a [`Navigator`]'s stack as a page.
///
/// Pages are built lazily from a factory on first visit; what happens to one afterwards is the destination's [`PagePolicy`], set with [`set_policy_for`](Self::set_policy_for). A persistent destination is filed under its route and reused forever (a rail item, a tab); a stack destination is filed under its stack entry, so pushing builds and popping releases, and the same route pushed twice is two screens. All built pages live in one layout container; only the active one is [`set_display`]ed, so the rest take no space. A navigation change is reconciled against the navigator whenever the host is asked for either half of itself — an event or a view — and drives the optional [`NavTransition`] on the incoming page. See `Seen` for why one of those was not enough.
pub struct NavHost<R: Clone + Eq + 'static> {
    nav: Navigator<R>,
    factory: Box<PageFactory<R>>,
    content_area: NodeId,
    seen: RefCell<Seen<R>>,
    transition: NavTransition,
    policy: Box<dyn Fn(&R) -> PagePolicy>,
    /// The owner every page is built under: one the host mints for itself, parented to whatever built the host.
    ///
    /// **Not whichever owner happens to be running when a page is wanted.** A page is built during a reconcile, and a reconcile can be reached from inside the page being left — so an ambient parent makes the incoming page a *child* of the outgoing one. [`prune`](Self::prune) then uproots the outgoing owner and every descendant with it, taking the new page's signals, effects and contexts while its layout nodes stay in the tree. What that leaves on screen is a page that draws and composes at full rate, and whose handlers still report events as handled, while nothing they set is read by anybody — a window that looks frozen with no error anywhere.
    ///
    /// Minting rather than capturing is the half that matters in an app: a root is mounted outside every scope, so a host that captured [`current_owner`](reactive_core::current_owner) would hold `None`, and building under `None` changes nothing — [`with_owner`] pushes no frame for it and the ambient owner stays current.
    ///
    /// A page's lifetime is the host's to decide, so the host is what owns it.
    owner: Option<OwnerId>,
}

impl<R: Clone + Eq + 'static> NavHost<R> {
    pub fn new(
        nav: Navigator<R>,
        factory: impl Fn(&R) -> Result<Box<dyn NavPage>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        let content_area = new_container(
            LayoutStyle::new()
                .flex_column()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[],
        )?;
        let root = nav.current();
        let prev_depth = nav.depth();
        let host = Self {
            nav,
            factory: Box::new(factory),
            content_area,
            seen: RefCell::new(Seen {
                pages: Vec::new(),
                current: root.clone(),
                current_key: PageKey::Route(root.clone()),
                prev_depth,
                entrance: None,
            }),
            transition: NavTransition::None,
            policy: Box::new(|_| PagePolicy::default()),
            // Minted here, not read off the ambient scope: mounting an app root runs outside every owner, so a host that only captured one would find `None` and go on parenting pages to the caller of the moment.
            owner: Some({
                let scope = owner_scope();
                let id = scope.id();
                drop(scope);
                id
            }),
        };
        let key = host.key_for(&root);
        host.seen.borrow_mut().current_key = key.clone();
        host.ensure_built(key)?;
        host.refresh_display();
        Ok(host)
    }

    /// Sets the animation played on the incoming page when navigation changes the current route.
    ///
    /// Takes `&mut self` rather than `self`: a [`TabHost`](crate::TabHost) mints one of these per tab and cannot take it by value, so a by-value builder would have been the second spelling of one setter.
    pub fn set_transition(&mut self, transition: NavTransition) {
        self.transition = transition;
    }

    /// Chooses the policy per destination, for the common host that serves both a fixed set of persistent destinations and screens pushed as a stack over them:
    ///
    /// ```ignore
    /// host.set_policy_for(|route| match route {
    ///     Route::Section(_) => PagePolicy::KeepAlive, // a rail item, kept as the reader left it
    ///     Route::Source(_) => PagePolicy::Transient,  // pushed detail: fresh per push, released on pop
    /// })
    /// ```
    pub fn set_policy_for(&mut self, policy: impl Fn(&R) -> PagePolicy + 'static) {
        self.policy = Box::new(policy);
        self.reseat_current_key();
    }

    /// Re-files the already-built root page under the key the new policy gives it, so a builder call after construction cannot leave the root unreachable.
    fn reseat_current_key(&mut self) {
        let key = self.key_for(&self.seen.borrow().current.clone());
        let mut seen = self.seen.borrow_mut();
        let was = seen.current_key.clone();
        if let Some(p) = seen.pages.iter_mut().find(|p| p.key == was) {
            p.key = key.clone();
        }
        seen.current_key = key;
    }

    /// The key a route's page is filed under right now: its stack slot when the destination is transient, the route itself when it is persistent. Only the top of the stack is ever displayed, so the slot is the current depth minus one.
    fn key_for(&self, route: &R) -> PageKey<R> {
        match (self.policy)(route) {
            PagePolicy::KeepAlive => PageKey::Route(route.clone()),
            PagePolicy::Transient => PageKey::Entry {
                slot: self.nav.peek_stack(|s| s.len().saturating_sub(1)),
                route: route.clone(),
            },
        }
    }

    /// The page filed under `key`, or nothing when it has not been built.
    ///
    /// Handed back as its own handle rather than an index into [`Seen::pages`], because what the caller does with it is call into it — and a page's own handler may navigate, which comes straight back here.
    fn page_at(&self, key: &PageKey<R>) -> Option<Rc<RefCell<Box<dyn NavPage>>>> {
        let seen = self.seen.borrow();
        let found = seen.pages.iter().find(|p| &p.key == key)?;
        Some(found.page.clone())
    }

    /// Builds and caches the page for `key` if it isn't already, appending its node to the container.
    fn ensure_built(&self, key: PageKey<R>) -> Result<(), LayoutError> {
        if self.page_at(&key).is_some() {
            return Ok(());
        }
        // Built under the host's own owner, and outside every borrow of `seen`: building a page runs the caller's code, and whatever it reads, writes or navigates must find this host as it always is.
        let (owner, page) = with_owner(self.owner, || {
            let scope = owner_scope();
            let owner = scope.id();
            let page = (self.factory)(key.route());
            drop(scope);
            (owner, page)
        });
        let page = page?;
        let node = page.layout_node();
        let mut seen = self.seen.borrow_mut();
        seen.pages.push(BuiltPage {
            key,
            page: Rc::new(RefCell::new(page)),
            node,
            owner,
        });
        let nodes: Vec<NodeId> = seen.pages.iter().map(|p| p.node).collect();
        drop(seen);
        set_children(self.content_area, &nodes)?;
        Ok(())
    }

    fn refresh_display(&self) {
        let seen = self.seen.borrow();
        for p in &seen.pages {
            set_display(p.node, p.key == seen.current_key);
        }
    }

    /// Makes `route` the displayed page: build it if needed, toggle visibility, mark the container dirty (the runner's `relayout_if_dirty` re-lays the host-owned root — never `compute_layout` here), start the entrance animation, and run the page's enter/relayout hooks.
    fn apply(&self, route: R) {
        let new_depth = self.nav.depth();
        let key = self.key_for(&route);
        if self.ensure_built(key.clone()).is_err() {
            return;
        }
        let forward = {
            let mut seen = self.seen.borrow_mut();
            let forward = new_depth >= seen.prev_depth;
            seen.prev_depth = new_depth;
            seen.current = route.clone();
            seen.current_key = key.clone();
            forward
        };
        self.refresh_display();
        mark_dirty(self.content_area).ok();

        if let Some(anim) = self.transition.start() {
            self.seen.borrow_mut().entrance = Some(Entrance {
                key: key.clone(),
                forward,
                anim,
            });
        }

        // Called with nothing of this host borrowed: a page's hooks are the caller's code, and `on_enter` moving the keyboard is exactly the sort of thing that comes back around.
        if let Some(page) = self.page_at(&key) {
            page.borrow_mut().on_relayout();
            page.borrow_mut().on_enter();
        }

        self.prune();
    }

    /// Releases every entry-keyed page whose stack entry is gone — popped past, replaced, or reset away. Route-keyed pages ([`PagePolicy::KeepAlive`]) belong to the host and are never pruned.
    ///
    /// Mirrors `ReactiveList`'s unmount order: detach the survivors first so a disposed node is out of the tree before it is freed, then free it, then let the page drop. That last drop is the point of the whole policy — it releases the page's widgets, and with them their signals and effects.
    fn prune(&self) {
        let live = self.nav.peek_stack(|s| s.to_vec());
        let mut seen = self.seen.borrow_mut();
        if seen.pages.iter().all(|p| p.key.is_live(&live)) {
            return;
        }
        let (keep, disposed): (Vec<_>, Vec<_>) = std::mem::take(&mut seen.pages)
            .into_iter()
            .partition(|p| p.key.is_live(&live));
        seen.pages = keep;
        let nodes: Vec<NodeId> = seen.pages.iter().map(|p| p.node).collect();
        drop(seen);
        set_children(self.content_area, &nodes).ok();
        // Dropped with nothing borrowed: what goes with a page is every widget it built, and a destructor is code like any other.
        for page in disposed {
            dispose_owner(page.owner);
            remove_node(page.node);
        }
    }

    /// Reconciles the displayed page against the navigator's current route, building and animating in the new page when they diverge.
    ///
    /// Both halves of [`Component`] call this — [`on_event`](Component::on_event) for a press dispatched through the tree, [`view`](Component::view) for every other way the navigator can move, an overlay's own handler among them (see `Seen`). It stays public for an owner that wants to know *whether* a press it handled itself moved the user — closing a mobile drawer on the ones that did — rather than because the reconcile needs asking for.
    ///
    /// Reports whether it actually navigated.
    pub fn sync(&mut self) -> bool {
        self.settle()
    }

    /// [`sync`](Self::sync) without the `&mut`, which is what lets the view settle before it renders.
    fn settle(&self) -> bool {
        let top = self.nav.current();
        // Compared by key, not by route: pushing the route already on screen is still a new stack entry, and under [`PagePolicy::Transient`] that entry gets its own page.
        if self.key_for(&top) == self.seen.borrow().current_key {
            return false;
        }
        self.apply(top);
        true
    }

    fn current_page(&self) -> Option<Rc<RefCell<Box<dyn NavPage>>>> {
        let key = self.seen.borrow().current_key.clone();
        self.page_at(&key)
    }

    /// Re-lay-out the active page's own scroll viewport(s). Forward this from the container's relayout.
    pub fn relayout(&mut self) {
        if let Some(page) = self.current_page() {
            page.borrow_mut().on_relayout();
        }
    }

    /// Run the active page's enter hook (autofocus). Forward this when the host first becomes visible.
    pub fn activate(&mut self) {
        if let Some(page) = self.current_page() {
            page.borrow_mut().on_enter();
        }
    }

    /// The current (top) route being displayed.
    pub fn current(&self) -> R {
        self.seen.borrow().current.clone()
    }

    /// Every page built so far, for the tests that count them and for no one else.
    #[cfg(test)]
    fn built(&self) -> std::cell::Ref<'_, Vec<BuiltPage<R>>> {
        std::cell::Ref::map(self.seen.borrow(), |seen| &seen.pages)
    }

    fn wrap_transition(&self, child: RenderNode, progress: f32, forward: bool) -> RenderNode {
        let width = absolute_rect(self.content_area)
            .map(|r| r.width)
            .unwrap_or(0.0);
        self.transition.wrap(child, progress, forward, width)
    }
}

impl<R: Clone + Eq + 'static> Component for NavHost<R> {
    fn view(&self) -> RenderNode {
        // Reconciled here as well as in `on_event`, and this is the half that catches everything else. Subscribing to the navigator makes this run again on every navigation, and settling first is what makes the page it renders the one the navigator names.
        self.settle();
        let Some(page) = self.current_page() else {
            return RenderNode::Empty;
        };
        let page_view = page.borrow().view();
        let seen = self.seen.borrow();
        if let Some(entrance) = &seen.entrance
            && entrance.key == seen.current_key
            && !entrance.anim.is_settled()
        {
            // Reading the animated value subscribes this view to the ticker, so it re-renders each frame until the page settles at its resting identity transform.
            let progress = entrance.anim.get();
            let forward = entrance.forward;
            drop(seen);
            return self.wrap_transition(page_view, progress, forward);
        }
        page_view
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Nothing of the host is borrowed while the page answers: a handler that navigates comes straight back through `settle`, and on an unbatched runtime it does so before this call has returned.
        let handled = match self.current_page() {
            Some(page) => page.borrow_mut().on_event(event),
            None => EventResult::Ignored,
        };
        self.settle();
        handled
    }
}

impl<R: Clone + Eq + 'static> LayoutItem for NavHost<R> {
    fn layout_node(&self) -> NodeId {
        self.content_area
    }
}

impl<R: Clone + Eq + 'static> Drop for NavHost<R> {
    /// Releases the owner [`new`](Self::new) minted, and every page owner under it. A host built outside any scope is a root with no ancestor to be uprooted by, so nothing else would ever free it.
    fn drop(&mut self) {
        if let Some(owner) = self.owner {
            dispose_owner(owner);
        }
    }
}

#[cfg(test)]
#[path = "host_test.rs"]
mod tests;
