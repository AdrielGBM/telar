use std::time::Duration;

use layout_core::{LayoutError, LayoutStyle, SizeDimension};
use motion_core::{Animated, Easing, tween};
use platform_core::Event;
use ui_core::{
    Component, EventResult, LayoutItem, NodeId, RenderNode, absolute_rect, mark_dirty,
    new_container, set_children, set_display,
};

use crate::navigator::Navigator;
use crate::page::NavPage;
use crate::transition::NavTransition;

const TRANSITION_MS: u64 = 220;

/// Builds a page for a route on its first visit — returns any [`NavPage`], or a [`LayoutError`] if the page's
/// widgets fail to construct.
type PageFactory<R> = dyn Fn(&R) -> Result<Box<dyn NavPage>, LayoutError>;

struct BuiltPage<R> {
    route: R,
    page: Box<dyn NavPage>,
    node: NodeId,
}

/// A running entrance animation for the page that just became active, plus its direction (forward push vs.
/// back pop) so a slide enters from the correct side.
struct Entrance<R> {
    route: R,
    forward: bool,
    anim: Animated<f32>,
}

/// A container that renders the top of a [`Navigator`]'s stack as a page.
///
/// Pages are built lazily from a factory the first time their route is navigated to, then cached and kept
/// alive — re-navigating to a route reuses its subtree (and its state) rather than rebuilding it. All built
/// pages live in one layout container; only the active one is [`set_display`]ed, so the rest take no space.
/// A navigation change is reconciled in [`on_event`](Component::on_event) — exactly like the runtime host's
/// tab switch — and drives the optional [`NavTransition`] on the incoming page.
pub struct NavHost<R: Clone + Eq + 'static> {
    nav: Navigator<R>,
    factory: Box<PageFactory<R>>,
    pages: Vec<BuiltPage<R>>,
    content_area: NodeId,
    /// The route currently displayed — the shadow reconciled against `nav.current()`.
    current: R,
    /// Stack depth at the last applied navigation, to tell a forward push from a back pop.
    prev_depth: usize,
    transition: NavTransition,
    entrance: Option<Entrance<R>>,
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
            prev_depth,
            transition: NavTransition::None,
            entrance: None,
        };
        host.ensure_built(&root)?;
        host.refresh_display();
        Ok(host)
    }

    /// Sets the animation played on the incoming page when navigation changes the current route.
    pub fn with_transition(mut self, transition: NavTransition) -> Self {
        self.transition = transition;
        self
    }

    fn index_of(&self, route: &R) -> Option<usize> {
        self.pages.iter().position(|p| &p.route == route)
    }

    /// Builds and caches the page for `route` if it isn't already, appending its node to the container.
    fn ensure_built(&mut self, route: &R) -> Result<usize, LayoutError> {
        if let Some(i) = self.index_of(route) {
            return Ok(i);
        }
        let page = (self.factory)(route)?;
        let node = page.layout_node();
        self.pages.push(BuiltPage {
            route: route.clone(),
            page,
            node,
        });
        let nodes: Vec<NodeId> = self.pages.iter().map(|p| p.node).collect();
        set_children(self.content_area, &nodes)?;
        Ok(self.pages.len() - 1)
    }

    fn refresh_display(&self) {
        for p in &self.pages {
            set_display(p.node, p.route == self.current);
        }
    }

    /// Makes `route` the displayed page: build it if needed, toggle visibility, mark the container dirty (the
    /// runner's `relayout_if_dirty` re-lays the host-owned root — never `compute_layout` here), start the
    /// entrance animation, and run the page's enter/relayout hooks.
    fn apply(&mut self, route: R) {
        let new_depth = self.nav.depth();
        let forward = new_depth >= self.prev_depth;
        self.prev_depth = new_depth;

        if self.ensure_built(&route).is_err() {
            return;
        }
        self.current = route.clone();
        self.refresh_display();
        mark_dirty(self.content_area).ok();

        if self.transition.is_animated() {
            let anim = Animated::new(
                0.0,
                tween(Duration::from_millis(TRANSITION_MS), Easing::EaseOut),
            );
            anim.retarget(1.0);
            self.entrance = Some(Entrance {
                route: route.clone(),
                forward,
                anim,
            });
        }

        if let Some(i) = self.index_of(&route) {
            self.pages[i].page.on_relayout();
            self.pages[i].page.on_enter();
        }
    }

    /// Re-lay-out the active page's own scroll viewport(s). Forward this from the container's relayout.
    pub fn relayout(&mut self) {
        if let Some(i) = self.index_of(&self.current) {
            self.pages[i].page.on_relayout();
        }
    }

    /// Run the active page's enter hook (autofocus). Forward this when the host first becomes visible.
    pub fn activate(&mut self) {
        if let Some(i) = self.index_of(&self.current) {
            self.pages[i].page.on_enter();
        }
    }

    /// The current (top) route being displayed.
    pub fn current(&self) -> R {
        self.current.clone()
    }

    fn wrap_transition(&self, child: RenderNode, progress: f32, forward: bool) -> RenderNode {
        let p = progress.clamp(0.0, 1.0);
        match self.transition {
            NavTransition::None => child,
            NavTransition::Fade => RenderNode::layer(p, 0.0, [child]),
            NavTransition::SlideHorizontal => {
                let width = absolute_rect(self.content_area)
                    .map(|r| r.width)
                    .unwrap_or(0.0);
                let dir = if forward { 1.0 } else { -1.0 };
                let dx = dir * width * (1.0 - p);
                RenderNode::transform_with([1.0, 0.0, 0.0, 1.0, dx, 0.0], [child])
            }
        }
    }
}

impl<R: Clone + Eq + 'static> Component for NavHost<R> {
    fn view(&self) -> RenderNode {
        // Subscribe to the navigator so this view re-renders on navigation. Event dispatch is batched
        // (`begin/end_event_batch` in the runner), so by the time this flushes, `on_event` has already run
        // `apply` — `self.current` is the new route and its page is built and displayed.
        let _subscribe = self.nav.current();
        let Some(i) = self.index_of(&self.current) else {
            return RenderNode::Empty;
        };
        let page_view = self.pages[i].page.view();
        if let Some(entrance) = &self.entrance
            && entrance.route == self.current
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
        let handled = if let Some(i) = self.index_of(&self.current) {
            self.pages[i].page.on_event(event)
        } else {
            EventResult::Ignored
        };
        let top = self.nav.current();
        if top != self.current {
            self.apply(top);
        }
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
