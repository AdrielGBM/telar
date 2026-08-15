use layout_core::{LayoutError, LayoutStyle, SizeDimension};
use motion_core::Animated;
use platform_core::Event;
use ui_core::{
    Component, EventResult, LayoutItem, NodeId, RenderNode, absolute_rect, mark_dirty,
    new_container, remove_node, set_children, set_display,
};

use crate::navigator::Navigator;
use crate::page::{NavPage, PagePolicy};
use crate::transition::NavTransition;

/// Builds a page for a route on its first visit — returns any [`NavPage`], or a [`LayoutError`] if the page's
/// widgets fail to construct.
type PageFactory<R> = dyn Fn(&R) -> Result<Box<dyn NavPage>, LayoutError>;

/// What a built page is filed under — the runtime form of [`PagePolicy`].
///
/// A stack only ever mutates at its top (`push`/`pop`/`replace`/`reset` truncate or touch the last entry), so an
/// entry never changes position while it lives: its index is a stable identity, with no per-entry id to mint or
/// to carry through a hot-reload snapshot. The route is part of the key so `replace`, which reuses the top index
/// for a different destination, still rebuilds.
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

    /// Whether the key still names a live stack entry. A route-keyed page always does — it belongs to the host,
    /// not to a position — while an entry-keyed one dies with its slot (popped past, replaced, or reset away).
    fn is_live(&self, stack: &[R]) -> bool {
        match self {
            PageKey::Route(_) => true,
            PageKey::Entry { slot, route } => stack.get(*slot) == Some(route),
        }
    }
}

struct BuiltPage<R> {
    key: PageKey<R>,
    page: Box<dyn NavPage>,
    node: NodeId,
}

/// A running entrance animation for the page that just became active, plus its direction (forward push vs.
/// back pop) so a slide enters from the correct side.
struct Entrance<R> {
    key: PageKey<R>,
    forward: bool,
    anim: Animated<f32>,
}

/// A container that renders the top of a [`Navigator`]'s stack as a page.
///
/// Pages are built lazily from a factory on first visit; what happens to one afterwards is the destination's
/// [`PagePolicy`], set with [`set_policy_for`](Self::set_policy_for).
/// A persistent destination is filed under its route and reused forever (a rail item, a tab); a stack
/// destination is filed under its stack entry, so pushing builds and popping releases, and the same route
/// pushed twice is two screens. All built pages live in one layout container; only the active one is
/// [`set_display`]ed, so the rest take no space. A navigation change is reconciled in
/// [`on_event`](Component::on_event) — exactly like the runtime host's tab switch — and drives the optional
/// [`NavTransition`] on the incoming page.
pub struct NavHost<R: Clone + Eq + 'static> {
    nav: Navigator<R>,
    factory: Box<PageFactory<R>>,
    pages: Vec<BuiltPage<R>>,
    content_area: NodeId,
    /// The route currently displayed — the shadow reconciled against `nav.current()`.
    current: R,
    /// Key of the displayed page: which of two pages for the same route is on screen, when the policy makes them
    /// distinct.
    current_key: PageKey<R>,
    /// Stack depth at the last applied navigation, to tell a forward push from a back pop.
    prev_depth: usize,
    transition: NavTransition,
    entrance: Option<Entrance<R>>,
    policy: Box<dyn Fn(&R) -> PagePolicy>,
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
        let mut host = Self {
            nav,
            factory: Box::new(factory),
            pages: Vec::new(),
            content_area,
            current: root.clone(),
            current_key: PageKey::Route(root.clone()),
            prev_depth,
            transition: NavTransition::None,
            entrance: None,
            policy: Box::new(|_| PagePolicy::default()),
        };
        host.current_key = host.key_for(&root);
        host.ensure_built(host.current_key.clone())?;
        host.refresh_display();
        Ok(host)
    }

    /// Sets the animation played on the incoming page when navigation changes the current route.
    pub fn with_transition(mut self, transition: NavTransition) -> Self {
        self.set_transition(transition);
        self
    }

    /// [`with_transition`](Self::with_transition) for a host that is already owned elsewhere — a
    /// [`TabHost`](crate::TabHost) mints one of these per tab and cannot take it by value.
    pub fn set_transition(&mut self, transition: NavTransition) {
        self.transition = transition;
    }

    /// Chooses the policy per destination, for the common host that serves both a fixed set of persistent
    /// destinations and screens pushed as a stack over them:
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

    /// Re-files the already-built root page under the key the new policy gives it, so a builder call after
    /// construction cannot leave the root unreachable.
    fn reseat_current_key(&mut self) {
        let key = self.key_for(&self.current.clone());
        if let Some(p) = self.pages.iter_mut().find(|p| p.key == self.current_key) {
            p.key = key.clone();
        }
        self.current_key = key;
    }

    /// The key a route's page is filed under right now: its stack slot when the destination is transient, the
    /// route itself when it is persistent. Only the top of the stack is ever displayed, so the slot is the
    /// current depth minus one.
    fn key_for(&self, route: &R) -> PageKey<R> {
        match (self.policy)(route) {
            PagePolicy::KeepAlive => PageKey::Route(route.clone()),
            PagePolicy::Transient => PageKey::Entry {
                slot: self.nav.peek_stack(|s| s.len().saturating_sub(1)),
                route: route.clone(),
            },
        }
    }

    fn index_of(&self, key: &PageKey<R>) -> Option<usize> {
        self.pages.iter().position(|p| &p.key == key)
    }

    /// Builds and caches the page for `key` if it isn't already, appending its node to the container.
    fn ensure_built(&mut self, key: PageKey<R>) -> Result<usize, LayoutError> {
        if let Some(i) = self.index_of(&key) {
            return Ok(i);
        }
        let page = (self.factory)(key.route())?;
        let node = page.layout_node();
        self.pages.push(BuiltPage { key, page, node });
        let nodes: Vec<NodeId> = self.pages.iter().map(|p| p.node).collect();
        set_children(self.content_area, &nodes)?;
        Ok(self.pages.len() - 1)
    }

    fn refresh_display(&self) {
        for p in &self.pages {
            set_display(p.node, p.key == self.current_key);
        }
    }

    /// Makes `route` the displayed page: build it if needed, toggle visibility, mark the container dirty (the
    /// runner's `relayout_if_dirty` re-lays the host-owned root — never `compute_layout` here), start the
    /// entrance animation, and run the page's enter/relayout hooks.
    fn apply(&mut self, route: R) {
        let new_depth = self.nav.depth();
        let forward = new_depth >= self.prev_depth;
        self.prev_depth = new_depth;

        let key = self.key_for(&route);
        if self.ensure_built(key.clone()).is_err() {
            return;
        }
        self.current = route.clone();
        self.current_key = key.clone();
        self.refresh_display();
        mark_dirty(self.content_area).ok();

        if let Some(anim) = self.transition.start() {
            self.entrance = Some(Entrance {
                key: key.clone(),
                forward,
                anim,
            });
        }

        if let Some(i) = self.index_of(&key) {
            self.pages[i].page.on_relayout();
            self.pages[i].page.on_enter();
        }

        self.prune();
    }

    /// Releases every entry-keyed page whose stack entry is gone — popped past, replaced, or reset away.
    /// Route-keyed pages ([`PagePolicy::KeepAlive`]) belong to the host and are never pruned.
    ///
    /// Mirrors `ReactiveList`'s unmount order: detach the survivors first so a disposed node is out of the
    /// tree before it is freed, then free it, then let the page drop. That last drop is the point of the whole
    /// policy — it releases the page's widgets, and with them their signals and effects.
    fn prune(&mut self) {
        let live = self.nav.peek_stack(|s| s.to_vec());
        if self.pages.iter().all(|p| p.key.is_live(&live)) {
            return;
        }
        let (keep, disposed): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pages)
            .into_iter()
            .partition(|p| p.key.is_live(&live));
        self.pages = keep;
        let nodes: Vec<NodeId> = self.pages.iter().map(|p| p.node).collect();
        set_children(self.content_area, &nodes).ok();
        for page in disposed {
            remove_node(page.node);
        }
    }

    /// Reconciles the displayed page against the navigator's current route, building and animating in the new
    /// page when they diverge.
    ///
    /// [`on_event`](Component::on_event) already calls this, which covers a press on a control inside a page.
    /// An owner whose navigation controls live *outside* the host's subtree — a shell sidebar that handles the
    /// press itself and never dispatches it into the host — must call this after that press instead.
    ///
    /// Reports whether it actually navigated, so such an owner can tell a press that moved the user (close the
    /// mobile drawer) from one that did not (a theme switch in the same sidebar).
    pub fn sync(&mut self) -> bool {
        let top = self.nav.current();
        // Compared by key, not by route: pushing the route already on screen is still a new stack entry, and
        // under [`PagePolicy::Transient`] that entry gets its own page.
        if self.key_for(&top) == self.current_key {
            return false;
        }
        self.apply(top);
        true
    }

    fn current_index(&self) -> Option<usize> {
        self.index_of(&self.current_key)
    }

    /// Re-lay-out the active page's own scroll viewport(s). Forward this from the container's relayout.
    pub fn relayout(&mut self) {
        if let Some(i) = self.current_index() {
            self.pages[i].page.on_relayout();
        }
    }

    /// Run the active page's enter hook (autofocus). Forward this when the host first becomes visible.
    pub fn activate(&mut self) {
        if let Some(i) = self.current_index() {
            self.pages[i].page.on_enter();
        }
    }

    /// The current (top) route being displayed.
    pub fn current(&self) -> R {
        self.current.clone()
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
        // Subscribe to the navigator so this view re-renders on navigation. Event dispatch is batched
        // (`begin/end_event_batch` in the runner), so by the time this flushes, `on_event` has already run
        // `apply` — `self.current` is the new route and its page is built and displayed.
        let _subscribe = self.nav.current();
        let Some(i) = self.current_index() else {
            return RenderNode::Empty;
        };
        let page_view = self.pages[i].page.view();
        if let Some(entrance) = &self.entrance
            && entrance.key == self.current_key
            && !entrance.anim.is_settled()
        {
            // Reading the animated value subscribes this view to the ticker, so it re-renders each frame until
            // the page settles at its resting identity transform.
            let progress = entrance.anim.get();
            return self.wrap_transition(page_view, progress, entrance.forward);
        }
        page_view
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let handled = if let Some(i) = self.current_index() {
            self.pages[i].page.on_event(event)
        } else {
            EventResult::Ignored
        };
        self.sync();
        handled
    }
}

impl<R: Clone + Eq + 'static> LayoutItem for NavHost<R> {
    fn layout_node(&self) -> NodeId {
        self.content_area
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use layout_core::{AvailableSpace, SizeDimension};
    use platform_core::Event;
    use reactive_core::{effect, signal};
    use ui_core::{absolute_rect, compute_layout, new_leaf, reset_layout_runtime};

    use super::*;

    type Log = Rc<RefCell<Vec<String>>>;

    struct TestPage {
        route: u8,
        node: NodeId,
        log: Log,
    }

    impl Component for TestPage {
        fn view(&self) -> RenderNode {
            RenderNode::Empty
        }
    }

    impl LayoutItem for TestPage {
        fn layout_node(&self) -> NodeId {
            self.node
        }
    }

    impl NavPage for TestPage {
        fn on_enter(&mut self) {
            self.log.borrow_mut().push(format!("enter:{}", self.route));
        }
        fn on_relayout(&mut self) {
            self.log
                .borrow_mut()
                .push(format!("relayout:{}", self.route));
        }
    }

    /// Records its route when dropped, so a test can observe which pages the host released.
    struct DropPage {
        route: u8,
        node: NodeId,
        dropped: Rc<RefCell<Vec<u8>>>,
    }

    impl Drop for DropPage {
        fn drop(&mut self) {
            self.dropped.borrow_mut().push(self.route);
        }
    }

    impl Component for DropPage {
        fn view(&self) -> RenderNode {
            RenderNode::Empty
        }
    }

    impl LayoutItem for DropPage {
        fn layout_node(&self) -> NodeId {
            self.node
        }
    }

    impl NavPage for DropPage {}

    /// Holds an effect for as long as the page lives, so dropping the page must release it.
    struct EffectPage {
        node: NodeId,
        _held: Option<reactive_core::Effect>,
    }

    impl Component for EffectPage {
        fn view(&self) -> RenderNode {
            RenderNode::Empty
        }
    }

    impl LayoutItem for EffectPage {
        fn layout_node(&self) -> NodeId {
            self.node
        }
    }

    impl NavPage for EffectPage {}

    struct Harness {
        built: Rc<RefCell<Vec<u8>>>,
        nodes: Rc<RefCell<Vec<(u8, NodeId)>>>,
        log: Log,
    }

    fn build(transition: NavTransition) -> (NavHost<u8>, Navigator<u8>, Harness) {
        reset_layout_runtime();
        let built = Rc::new(RefCell::new(Vec::new()));
        let nodes = Rc::new(RefCell::new(Vec::new()));
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let nav = Navigator::new(0u8);
        let factory = {
            let (built, nodes, log) = (built.clone(), nodes.clone(), log.clone());
            move |route: &u8| {
                built.borrow_mut().push(*route);
                let (node, _rect) = new_leaf(
                    LayoutStyle::new()
                        .width(SizeDimension::Percent(1.0))
                        .height(SizeDimension::Percent(1.0)),
                )?;
                nodes.borrow_mut().push((*route, node));
                Ok(Box::new(TestPage {
                    route: *route,
                    node,
                    log: log.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        let host = NavHost::new(nav.clone(), factory)
            .unwrap()
            .with_transition(transition);
        (host, nav, Harness { built, nodes, log })
    }

    fn tick(host: &mut NavHost<u8>) {
        host.on_event(&Event::CursorEntered);
    }

    fn node_of(h: &Harness, route: u8) -> NodeId {
        h.nodes
            .borrow()
            .iter()
            .find(|(r, _)| *r == route)
            .map(|(_, n)| *n)
            .unwrap()
    }

    #[test]
    fn builds_root_lazily_and_caches_pages() {
        let (mut host, nav, h) = build(NavTransition::None);
        assert_eq!(
            *h.built.borrow(),
            vec![0],
            "only the root page is built up front"
        );
        assert_eq!(host.current(), 0);

        // A benign event with no navigation change rebuilds nothing.
        tick(&mut host);
        assert_eq!(*h.built.borrow(), vec![0]);

        nav.push(1);
        tick(&mut host);
        assert_eq!(
            *h.built.borrow(),
            vec![0, 1],
            "the pushed route is built on first visit"
        );
        assert_eq!(host.current(), 1);

        nav.pop();
        tick(&mut host);
        assert_eq!(
            *h.built.borrow(),
            vec![0, 1],
            "returning to a route reuses its cached page"
        );
        assert_eq!(host.current(), 0);
    }

    #[test]
    fn keep_alive_retains_pages_popped_past() {
        let (mut host, nav, h) = build(NavTransition::None);
        nav.push(1);
        tick(&mut host);
        nav.pop();
        tick(&mut host);
        assert_eq!(host.pages.len(), 2, "the popped page is still cached");
        nav.push(1);
        tick(&mut host);
        assert_eq!(
            *h.built.borrow(),
            vec![0, 1],
            "revisiting reuses the cached page rather than rebuilding it"
        );
    }

    /// `KeepAlive` files a page under its route, so a stack naming the same route twice shares one page — one
    /// scroll position, one set of widget state — between both positions. That is what a persistent destination
    /// means, and the opposite of what a page stack does (see the `Transient` case below).
    #[test]
    fn keep_alive_shares_one_page_between_two_stack_entries() {
        let (mut host, nav, h) = build(NavTransition::None);
        nav.push(1);
        tick(&mut host);
        nav.push(0);
        tick(&mut host);
        assert_eq!(nav.depth(), 3);
        assert_eq!(
            *h.built.borrow(),
            vec![0, 1],
            "route 0 was not built a second time"
        );
        assert_eq!(host.pages.len(), 2);
        assert_eq!(node_of(&h, 0), host.pages[0].node);
    }

    /// `Transient` files a page under its stack entry, which is what makes it a real page stack: the same route
    /// pushed at two depths is two independent screens, and popping one releases only that one.
    #[test]
    fn transient_gives_each_stack_entry_its_own_page() {
        reset_layout_runtime();
        let built = Rc::new(RefCell::new(Vec::new()));
        let dropped: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let nav = Navigator::new(0u8);
        let factory = {
            let (built, dropped) = (built.clone(), dropped.clone());
            move |route: &u8| {
                built.borrow_mut().push(*route);
                let (node, _rect) = new_leaf(LayoutStyle::new())?;
                Ok(Box::new(DropPage {
                    route: *route,
                    node,
                    dropped: dropped.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        let mut host = NavHost::new(nav.clone(), factory).unwrap();
        host.set_policy_for(|_| PagePolicy::Transient);

        nav.push(1);
        tick(&mut host);
        nav.push(0);
        tick(&mut host);
        assert_eq!(
            *built.borrow(),
            vec![0, 1, 0],
            "the repeated route was built again for its own entry"
        );
        assert_eq!(host.pages.len(), 3);
        assert!(dropped.borrow().is_empty());

        nav.pop();
        tick(&mut host);
        assert_eq!(
            *dropped.borrow(),
            vec![0],
            "only the popped entry's page was released"
        );
        assert_eq!(
            host.pages.len(),
            2,
            "the root's own page for route 0 is untouched"
        );
        assert_eq!(host.current(), 1);
    }

    /// A `replace` reuses the top stack slot for a different route, so the entry-keyed page filed under that slot
    /// must be released rather than left behind for a screen the user can no longer reach.
    #[test]
    fn transient_releases_a_replaced_entry() {
        reset_layout_runtime();
        let dropped: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let nav = Navigator::new(0u8);
        let factory = {
            let dropped = dropped.clone();
            move |route: &u8| {
                let (node, _rect) = new_leaf(LayoutStyle::new())?;
                Ok(Box::new(DropPage {
                    route: *route,
                    node,
                    dropped: dropped.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        let mut host = NavHost::new(nav.clone(), factory).unwrap();
        host.set_policy_for(|_| PagePolicy::Transient);

        nav.push(1);
        tick(&mut host);
        nav.replace(2);
        tick(&mut host);
        assert_eq!(host.current(), 2);
        assert_eq!(*dropped.borrow(), vec![1], "the replaced page was released");
    }

    /// The mixed host the policy-per-destination API exists for: rail destinations kept as the user left them,
    /// with a detail pushed over them that is fresh per push and released on the way back.
    #[test]
    fn a_persistent_destination_and_a_pushed_detail_coexist() {
        reset_layout_runtime();
        let built = Rc::new(RefCell::new(Vec::new()));
        let dropped: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let nav = Navigator::new(0u8);
        let factory = {
            let (built, dropped) = (built.clone(), dropped.clone());
            move |route: &u8| {
                built.borrow_mut().push(*route);
                let (node, _rect) = new_leaf(LayoutStyle::new())?;
                Ok(Box::new(DropPage {
                    route: *route,
                    node,
                    dropped: dropped.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        // Even routes are rail destinations, odd routes are pushed details.
        let mut host = NavHost::new(nav.clone(), factory).unwrap();
        host.set_policy_for(|route: &u8| {
            if route % 2 == 0 {
                PagePolicy::KeepAlive
            } else {
                PagePolicy::Transient
            }
        });

        nav.push(1);
        tick(&mut host);
        nav.pop();
        tick(&mut host);
        assert_eq!(
            *dropped.borrow(),
            vec![1],
            "the detail was released on the way back"
        );
        nav.push(1);
        tick(&mut host);
        assert_eq!(
            *built.borrow(),
            vec![0, 1, 1],
            "pushing the detail again builds it fresh"
        );

        // The rail destination it was pushed over survived all of it, and revisiting reuses it.
        nav.pop();
        nav.push(2);
        tick(&mut host);
        nav.pop();
        tick(&mut host);
        assert_eq!(*built.borrow(), vec![0, 1, 1, 2]);
        assert!(
            !dropped.borrow().contains(&0) && !dropped.borrow().contains(&2),
            "rail destinations are never released: {:?}",
            dropped.borrow()
        );
    }

    #[test]
    fn transient_drops_pages_popped_past_but_keeps_the_stack() {
        reset_layout_runtime();
        let built = Rc::new(RefCell::new(Vec::new()));
        let dropped: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let nav = Navigator::new(0u8);
        let factory = {
            let (built, dropped) = (built.clone(), dropped.clone());
            move |route: &u8| {
                built.borrow_mut().push(*route);
                let (node, _rect) = new_leaf(LayoutStyle::new())?;
                Ok(Box::new(DropPage {
                    route: *route,
                    node,
                    dropped: dropped.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        let mut host = NavHost::new(nav.clone(), factory).unwrap();
        host.set_policy_for(|_| PagePolicy::Transient);

        nav.push(1);
        tick(&mut host);
        nav.push(2);
        tick(&mut host);
        assert!(
            dropped.borrow().is_empty(),
            "everything is still on the stack, so nothing is released"
        );

        // Back to the root in one step: 1 and 2 are popped past and must go, the root must not.
        nav.pop_to_root();
        tick(&mut host);
        let mut gone = dropped.borrow().clone();
        gone.sort();
        assert_eq!(gone, vec![1, 2], "only the popped pages were released");
        assert_eq!(host.pages.len(), 1, "the root page is still built");
        assert_eq!(host.current(), 0);

        // Revisiting a released route rebuilds it, rather than resurrecting a stale subtree.
        nav.push(1);
        tick(&mut host);
        assert_eq!(*built.borrow(), vec![0, 1, 2, 1]);
    }

    #[test]
    fn transient_teardown_releases_the_pages_effects() {
        reset_layout_runtime();
        let source = signal(0i32);
        let runs = Rc::new(RefCell::new(0usize));
        let nav = Navigator::new(0u8);
        let factory = {
            let (source, runs) = (source.clone(), runs.clone());
            move |route: &u8| {
                let (node, _rect) = new_leaf(LayoutStyle::new())?;
                // Only the pushed page owns an effect, so the counter tracks exactly that page's lifetime.
                let held = (*route == 1).then(|| {
                    let (source, runs) = (source.clone(), runs.clone());
                    effect(move || {
                        source.get();
                        *runs.borrow_mut() += 1;
                    })
                });
                Ok(Box::new(EffectPage { node, _held: held }) as Box<dyn NavPage>)
            }
        };
        let mut host = NavHost::new(nav.clone(), factory).unwrap();
        host.set_policy_for(|_| PagePolicy::Transient);

        nav.push(1);
        tick(&mut host);
        let after_mount = *runs.borrow();
        source.set(1);
        assert_eq!(
            *runs.borrow(),
            after_mount + 1,
            "the mounted page's effect re-runs on a source change"
        );

        nav.pop();
        tick(&mut host);
        let after_teardown = *runs.borrow();
        source.set(2);
        assert_eq!(
            *runs.borrow(),
            after_teardown,
            "dropping the page released its effect — the cascade navigate exists to get"
        );
    }

    #[test]
    fn sync_reconciles_without_an_event() {
        let (mut host, nav, h) = build(NavTransition::None);
        nav.push(1);
        // A shell that handles the navigation press itself never dispatches into the host, so no event arrives.
        host.sync();
        assert_eq!(*h.built.borrow(), vec![0, 1]);
        assert_eq!(host.current(), 1);

        // Already reconciled: a redundant sync neither rebuilds nor re-enters.
        h.log.borrow_mut().clear();
        host.sync();
        assert_eq!(*h.built.borrow(), vec![0, 1]);
        assert!(h.log.borrow().is_empty());
    }

    #[test]
    fn runs_lifecycle_hooks_on_the_active_page() {
        let (mut host, nav, h) = build(NavTransition::None);
        host.activate();
        assert_eq!(*h.log.borrow(), vec!["enter:0"]);

        nav.push(1);
        tick(&mut host);
        assert_eq!(
            *h.log.borrow(),
            vec!["enter:0", "relayout:1", "enter:1"],
            "the newly active page relays out then enters"
        );
    }

    #[test]
    fn displays_only_the_active_page() {
        let (mut host, nav, h) = build(NavTransition::None);
        nav.push(1);
        tick(&mut host);
        compute_layout(
            host.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .ok();
        let active = absolute_rect(node_of(&h, 1))
            .map(|r| r.width)
            .unwrap_or(0.0);
        let hidden = absolute_rect(node_of(&h, 0))
            .map(|r| r.width)
            .unwrap_or(0.0);
        assert!(active > 0.0, "the active page fills the host");
        assert_eq!(hidden, 0.0, "the inactive page is collapsed out of layout");
    }

    #[test]
    fn transition_wraps_the_incoming_page_while_animating() {
        let (mut host, nav, _h) = build(NavTransition::SlideHorizontal);
        // No animation at rest: the root page renders bare.
        assert!(matches!(host.view(), RenderNode::Empty));

        nav.push(1);
        tick(&mut host);
        // A just-started slide wraps the page in a transform until it settles.
        assert!(matches!(host.view(), RenderNode::Transform { .. }));

        let (mut fade_host, fade_nav, _h2) = build(NavTransition::Fade);
        fade_nav.push(1);
        tick(&mut fade_host);
        assert!(matches!(fade_host.view(), RenderNode::Layer { .. }));
    }
}
