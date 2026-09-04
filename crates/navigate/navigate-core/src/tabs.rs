//! Tabs: one route stack per tab, and the host that shows whichever tab is active.

use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, SizeDimension};
use motion_core::Animated;
use platform_core::Event;
use reactive_core::{RwSignal, signal};
use ui_core::{
    Component, EventResult, LayoutItem, NodeId, RenderNode, absolute_rect, mark_dirty,
    new_container, set_children, set_display,
};

use crate::host::NavHost;
use crate::navigator::Navigator;
use crate::page::{NavPage, PagePolicy};
use crate::transition::NavTransition;

/// Builds a page for a route inside a given tab. The tab is passed alongside the route so an app whose route type is shared between tabs can still tell them apart; an app whose routes already name their tab can ignore it.
type TabPageFactory<T, R> = dyn Fn(&T, &R) -> Result<Box<dyn NavPage>, LayoutError>;

/// One tab's navigation stack, plus the [`NavHost`] rendering it — minted on the tab's first visit, then kept for the life of the host. That is the whole point of nested stacks: a tab you leave is still standing where you left it, several screens deep, without any per-route keep-alive policy holding it up.
struct BuiltTab<T, R: Clone + Eq + 'static> {
    tab: T,
    host: NavHost<R>,
    node: NodeId,
}

/// One navigation stack per tab, and which tab is active — the state half of [`TabHost`], and the handle the app's tab bar or rail drives.
///
/// A single [`Navigator`] is a browser: one shared history that every destination pushes onto. This is the native model instead — `UITabBarController` giving each tab its own `UINavigationController`, Flutter's nested `Navigator`, Compose's nested graph — where switching tabs is not a navigation at all: it swaps which stack you are looking at, and each tab keeps its own depth and its own history.
///
/// Cheap to clone (the active-tab signal and the stack table are refcounted), so the rail that selects tabs, the back control, and the pages that push all hold their own handle.
pub struct TabStacks<T: Clone + Eq + 'static, R: Clone + 'static> {
    active: RwSignal<T>,
    navs: Rc<Vec<(T, Navigator<R>)>>,
    /// Tabs visited before the active one, oldest first — `None` when tab history is off (the default). A signal rather than a plain cell so a Back control's enabled state tracks it without extra wiring.
    history: Option<RwSignal<Vec<T>>>,
}

impl<T: Clone + Eq + 'static, R: Clone + 'static> Clone for TabStacks<T, R> {
    fn clone(&self) -> Self {
        Self {
            active: self.active,
            navs: self.navs.clone(),
            history: self.history,
        }
    }
}

/// How many previous tabs a history keeps. Deep enough that a reading session never notices the bound, small enough that it cannot grow without limit over hours of use.
const TAB_HISTORY_LIMIT: usize = 32;

impl<T: Clone + Eq + 'static, R: Clone + 'static> TabStacks<T, R> {
    /// Mints one stack per entry of `tabs`, in that order, with `active` naming the one on screen.
    ///
    /// `navigator_for` builds each tab's stack: `Navigator::new(root)` for a plain one, or `Navigator::from_signal(hot_signal(key, vec![root]), root)` to keep that tab's history across a hot-reload dylib swap. Minting every stack up front is cheap — a stack is one signal holding a `Vec<R>`; it is the *pages* that are deferred, and [`TabHost`] still builds those on a tab's first visit.
    ///
    /// `active` is clamped into the set, so a tab restored from a hot snapshot (or a deep link) that no longer exists falls back to the first tab rather than leaving the host with no stack to show. An empty `tabs` is read as "just the active one", which keeps the never-empty invariant without a fallible constructor.
    pub fn new(
        active: RwSignal<T>,
        tabs: &[T],
        navigator_for: impl Fn(&T) -> Navigator<R>,
    ) -> Self {
        let owned: Vec<T> = if tabs.is_empty() {
            vec![active.peek()]
        } else {
            tabs.to_vec()
        };
        if !owned.iter().any(|t| *t == active.peek()) {
            active.set(owned[0].clone());
        }
        let navs = owned
            .into_iter()
            .map(|tab| {
                let nav = navigator_for(&tab);
                (tab, nav)
            })
            .collect();
        Self {
            active,
            navs: Rc::new(navs),
            history: None,
        }
    }

    /// Lets [`back`](Self::back) leave the active tab: once its stack is at its root, the next Back returns to the tab you were on before, and so on until there is nothing left to go back to.
    ///
    /// Off by default, which is iOS tab-bar semantics — tabs are parallel contexts and Back never crosses them. Turn it on for Android (where the platform back button is expected to walk back through tabs, and only then leave the app) and for any rail that reads as a table of contents rather than a tab bar, where "go back to what I was just reading" is what a user means by Back.
    pub fn with_tab_history(mut self) -> Self {
        self.history = Some(signal(Vec::new()));
        self
    }

    /// Records `leaving` as the tab to come back to. Consecutive duplicates are impossible (a switch is only recorded when the tab actually changes), and the history is capped so a long session cannot grow it without bound.
    pub(crate) fn remember(&self, leaving: T) {
        let Some(history) = &self.history else {
            return;
        };
        history.update(|h| {
            h.push(leaving);
            if h.len() > TAB_HISTORY_LIMIT {
                h.remove(0);
            }
        });
    }

    /// Returns to the previously visited tab, if tab history is on and holds one. The tab being left is *not* recorded, or Back would bounce between two tabs forever instead of walking out.
    fn pop_tab_history(&self) -> bool {
        let Some(history) = &self.history else {
            return false;
        };
        // Read under its own borrow: `update` re-enters the signal, and writing `active` below flushes effects.
        let Some(previous) = history.peek_with(|h| h.last().cloned()) else {
            return false;
        };
        history.update(|h| {
            h.pop();
        });
        self.active.set(previous);
        true
    }

    /// Reactive read of the active tab — what a rail item reads to highlight itself.
    pub fn active(&self) -> T {
        self.active.get()
    }

    /// Non-subscribing read of the active tab, for use inside an event handler.
    pub fn peek_active(&self) -> T {
        self.active.peek()
    }

    /// The backing signal, so an app can make the active tab hot-preserved state (`hot_signal`) or drive it from somewhere else entirely.
    pub fn active_signal(&self) -> RwSignal<T> {
        self.active
    }

    /// Every tab, in the order they were declared.
    pub fn tabs(&self) -> impl Iterator<Item = &T> {
        self.navs.iter().map(|(tab, _)| tab)
    }

    /// Selects `tab`, or — when it is already the active one — pops its stack back to its root.
    ///
    /// That second behaviour is the platform convention (tapping the tab you are already on returns you to the top of it), and it is what makes a tab bar item never inert: pressing it from three screens deep inside that tab takes you home rather than doing nothing.
    pub fn select(&self, tab: T) {
        let leaving = self.active.peek();
        if leaving == tab {
            if let Some(nav) = self.navigator_for(&tab) {
                nav.pop_to_root();
            }
            return;
        }
        self.remember(leaving);
        self.active.set(tab);
    }

    /// The stack belonging to `tab`, or `None` when it names no declared tab.
    pub fn navigator_for(&self, tab: &T) -> Option<Navigator<R>> {
        self.navs
            .iter()
            .find(|(t, _)| t == tab)
            .map(|(_, nav)| nav.clone())
    }

    /// The active tab's stack — what a control inside a page pushes onto.
    pub fn navigator(&self) -> Navigator<R> {
        let active = self.active.peek();
        self.navigator_for(&active)
            .unwrap_or_else(|| self.navs[0].1.clone())
    }

    /// Pushes a screen onto the active tab's stack.
    pub fn push(&self, route: R) {
        self.navigator().push(route);
    }

    /// One "back" as the user means it: closes the frontmost dialog if one is open, else pops the active tab's stack, else — only with [`with_tab_history`](Self::with_tab_history) — returns to the previous tab. Reports whether anything happened, so a hardware back gesture can fall through to the OS when it did not.
    ///
    /// Without tab history, Back stays strictly *within* a tab, which is the difference between a tab bar and browser history.
    pub fn back(&self) -> bool {
        if ui_core::dismiss::dismiss_top() {
            return true;
        }
        if self.navigator().pop() {
            return true;
        }
        self.pop_tab_history()
    }

    /// Reactive read of whether [`back`](Self::back) would move the user: a screen to pop in the active tab, or — with tab history on — a tab to return to. Does *not* count an open dialog; read `use_dismiss_depth` alongside this for a control that lights up for that too.
    pub fn can_pop(&self) -> bool {
        // Subscribe to the tab as well: switching tabs changes which stack this answer comes from.
        let active = self.active.get();
        let in_tab = self.navigator_for(&active).is_some_and(|nav| nav.can_pop());
        in_tab
            || self
                .history
                .as_ref()
                .is_some_and(|h| h.with(|entries| !entries.is_empty()))
    }

    /// Reactive read of the active tab's stack depth (`1` at its root).
    pub fn depth(&self) -> usize {
        let active = self.active.get();
        self.navigator_for(&active).map_or(1, |nav| nav.depth())
    }
}

/// A container that renders the active tab's stack, with one [`NavHost`] per tab.
///
/// Where [`NavHost`] shows the top of *one* stack, this shows the top of *the active tab's* stack, and holds the other tabs' hosts alongside it — built on first visit, then collapsed out of layout with [`set_display`] while another tab is on screen. Nothing is torn down on a tab switch, so a tab that was three screens deep with a half-filled form is exactly that when you come back to it.
///
/// The [`PagePolicy`] and [`NavTransition`] set here apply to every tab's host, so `Transient` — the real page stack semantics — is the sane default choice here in a way it never was for a single shared stack: each tab's persistence comes from its stack staying alive, not from pinning pages by route.
pub struct TabHost<T: Clone + Eq + 'static, R: Clone + Eq + 'static> {
    stacks: TabStacks<T, R>,
    factory: Rc<TabPageFactory<T, R>>,
    policy: Rc<dyn Fn(&R) -> PagePolicy>,
    transition: NavTransition,
    tab_transition: NavTransition,
    tabs: Vec<BuiltTab<T, R>>,
    content_area: NodeId,
    /// The tab currently displayed — the shadow reconciled against `stacks.active()`.
    current: T,
    /// A running entrance animation for the tab just switched to, plus whether it sits later in declaration order than the one it replaced (so a slide enters from the side the tab bar suggests).
    entrance: Option<(T, bool, Animated<f32>)>,
}

impl<T: Clone + Eq + 'static, R: Clone + Eq + 'static> TabHost<T, R> {
    pub fn new(
        stacks: TabStacks<T, R>,
        factory: impl Fn(&T, &R) -> Result<Box<dyn NavPage>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        let content_area = new_container(
            LayoutStyle::new()
                .flex_column()
                .flex_grow(1.0)
                .width(SizeDimension::Percent(1.0)),
            &[],
        )?;
        let current = stacks.peek_active();
        let mut host = Self {
            stacks,
            factory: Rc::new(factory),
            policy: Rc::new(|_| PagePolicy::default()),
            transition: NavTransition::None,
            tab_transition: NavTransition::None,
            tabs: Vec::new(),
            content_area,
            current: current.clone(),
            entrance: None,
        };
        host.ensure_built(&current)?;
        host.refresh_display();
        Ok(host)
    }

    /// Sets the animation played on the incoming page when navigation moves *within* a tab.
    pub fn with_transition(mut self, transition: NavTransition) -> Self {
        self.transition = transition;
        for built in &mut self.tabs {
            built.host.set_transition(transition);
        }
        self
    }

    /// Sets the animation played on the incoming tab when a different one is selected. Defaults to [`NavTransition::None`], which is what a real tab bar does — selecting a tab swaps context rather than travelling somewhere, and animating it makes a fast switcher feel sluggish. Worth turning on for a rail that reads more like a table of contents than a tab bar.
    pub fn with_tab_transition(mut self, transition: NavTransition) -> Self {
        self.tab_transition = transition;
        self
    }

    /// Applies one page policy inside every tab. Defaults to [`PagePolicy::KeepAlive`].
    pub fn with_policy(self, policy: PagePolicy) -> Self {
        self.with_policy_for(move |_| policy)
    }

    /// Chooses the page policy per destination, as [`NavHost::set_policy_for`](crate::NavHost::set_policy_for) does, applied inside every tab.
    pub fn with_policy_for(mut self, policy: impl Fn(&R) -> PagePolicy + 'static) -> Self {
        self.policy = Rc::new(policy);
        for built in &mut self.tabs {
            let policy = self.policy.clone();
            built.host.set_policy_for(move |route| policy(route));
        }
        self
    }

    /// The per-tab stacks this host renders — clone it to drive the tab bar or a back control.
    pub fn stacks(&self) -> TabStacks<T, R> {
        self.stacks.clone()
    }

    fn index_of(&self, tab: &T) -> Option<usize> {
        self.tabs.iter().position(|built| &built.tab == tab)
    }

    fn current_index(&self) -> Option<usize> {
        self.index_of(&self.current)
    }

    /// Where a tab sits in declaration order — the tab bar's own left-to-right order, which is the only thing a directional switch animation can key off (tabs have no depth to compare).
    fn order_of(&self, tab: &T) -> usize {
        self.stacks.tabs().position(|t| t == tab).unwrap_or(0)
    }

    /// Builds `tab`'s host if this is its first visit, appending its node to the container. Deferring this is what keeps an app with twenty tabs from constructing twenty screens at boot.
    fn ensure_built(&mut self, tab: &T) -> Result<usize, LayoutError> {
        if let Some(i) = self.index_of(tab) {
            return Ok(i);
        }
        let Some(nav) = self.stacks.navigator_for(tab) else {
            return Err(LayoutError::Engine(
                "TabHost: the active tab names no declared stack".into(),
            ));
        };
        let factory = self.factory.clone();
        let owner = tab.clone();
        let mut host = NavHost::new(nav, move |route: &R| factory(&owner, route))?;
        host.set_transition(self.transition);
        let policy = self.policy.clone();
        host.set_policy_for(move |route| policy(route));
        let node = host.layout_node();
        self.tabs.push(BuiltTab {
            tab: tab.clone(),
            host,
            node,
        });
        let nodes: Vec<NodeId> = self.tabs.iter().map(|built| built.node).collect();
        set_children(self.content_area, &nodes)?;
        Ok(self.tabs.len() - 1)
    }

    fn refresh_display(&self) {
        for built in &self.tabs {
            set_display(built.node, built.tab == self.current);
        }
    }

    /// Reconciles the displayed tab against the active one, and the displayed page against that tab's stack.
    ///
    /// [`on_event`](Component::on_event) already calls this. An owner whose tab bar lives *outside* this host's subtree — a shell rail that handles the press itself — must call it after that press, exactly as with [`NavHost::sync`], and gets back whether the press actually moved the user.
    pub fn sync(&mut self) -> bool {
        let active = self.stacks.peek_active();
        let switched = active != self.current;
        if switched {
            if self.ensure_built(&active).is_err() {
                return false;
            }
            let forward = self.order_of(&active) >= self.order_of(&self.current);
            self.current = active;
            self.refresh_display();
            mark_dirty(self.content_area).ok();
            if let Some(anim) = self.tab_transition.start() {
                self.entrance = Some((self.current.clone(), forward, anim));
            }
        }
        let Some(i) = self.current_index() else {
            return switched;
        };
        // A background tab's stack can move while it is off screen (a deep link, a guard redirect), so the incoming tab reconciles its own stack on the way in.
        let navigated = self.tabs[i].host.sync();
        if switched && !navigated {
            // An unchanged stack still needs its page re-laid and re-entered: it was collapsed out of layout until a moment ago, so its scroll viewport has only now been given a size.
            self.tabs[i].host.relayout();
            self.tabs[i].host.activate();
        }
        switched || navigated
    }

    /// Re-lay-out the active page's own scroll viewport(s). Forward this from the container's relayout.
    pub fn relayout(&mut self) {
        if let Some(i) = self.current_index() {
            self.tabs[i].host.relayout();
        }
    }

    /// Run the active page's enter hook (autofocus). Forward this when the host first becomes visible.
    pub fn activate(&mut self) {
        if let Some(i) = self.current_index() {
            self.tabs[i].host.activate();
        }
    }

    /// The tab currently on screen.
    pub fn current_tab(&self) -> T {
        self.current.clone()
    }

    /// The route on top of the active tab's stack.
    pub fn current_route(&self) -> Option<R> {
        self.current_index().map(|i| self.tabs[i].host.current())
    }
}

impl<T: Clone + Eq + 'static, R: Clone + Eq + 'static> Component for TabHost<T, R> {
    fn view(&self) -> RenderNode {
        // Subscribe to the active tab so this view re-renders on a switch; the per-tab host subscribes to its own stack, which re-subscribes on each switch because only the active one is rendered.
        let _subscribe = self.stacks.active();
        let Some(i) = self.current_index() else {
            return RenderNode::Empty;
        };
        let tab_view = self.tabs[i].host.view();
        if let Some((tab, forward, anim)) = &self.entrance
            && *tab == self.current
            && !anim.is_settled()
        {
            // Reading the animated value subscribes this view to the ticker, so it re-renders each frame until the incoming tab settles at its resting identity transform.
            let progress = anim.get();
            let width = absolute_rect(self.content_area)
                .map(|r| r.width)
                .unwrap_or(0.0);
            return self
                .tab_transition
                .wrap(tab_view, progress, *forward, width);
        }
        tab_view
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let handled = match self.current_index() {
            Some(i) => self.tabs[i].host.on_event(event),
            None => EventResult::Ignored,
        };
        self.sync();
        handled
    }
}

impl<T: Clone + Eq + 'static, R: Clone + Eq + 'static> LayoutItem for TabHost<T, R> {
    fn layout_node(&self) -> NodeId {
        self.content_area
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use reactive_core::signal;
    use ui_core::{absolute_rect, compute_layout, new_leaf, reset_layout_runtime};

    use super::*;

    /// Which screen a page is: the tab it belongs to, and the route within that tab's stack.
    type PageId = (u8, u8);
    type PageLog = Rc<RefCell<Vec<PageId>>>;
    type NodeLog = Rc<RefCell<Vec<(PageId, NodeId)>>>;

    /// Records its (tab, route) when built and when dropped, so a test can observe exactly which screens the host constructed and which it released.
    struct TestPage {
        id: PageId,
        node: NodeId,
        dropped: PageLog,
    }

    impl Drop for TestPage {
        fn drop(&mut self) {
            self.dropped.borrow_mut().push(self.id);
        }
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

    impl NavPage for TestPage {}

    struct Harness {
        built: PageLog,
        dropped: PageLog,
        nodes: NodeLog,
    }

    /// Three tabs (`0`, `1`, `2`), each rooted at the route equal to its own index.
    fn build() -> (TabHost<u8, u8>, TabStacks<u8, u8>, Harness) {
        reset_layout_runtime();
        let built = Rc::new(RefCell::new(Vec::new()));
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let nodes = Rc::new(RefCell::new(Vec::new()));
        let stacks = TabStacks::new(signal(0u8), &[0, 1, 2], |tab| Navigator::new(*tab));
        let factory = {
            let (built, dropped, nodes) = (built.clone(), dropped.clone(), nodes.clone());
            move |tab: &u8, route: &u8| {
                let id = (*tab, *route);
                built.borrow_mut().push(id);
                let (node, _rect) = new_leaf(
                    LayoutStyle::new()
                        .width(SizeDimension::Percent(1.0))
                        .height(SizeDimension::Percent(1.0)),
                )?;
                nodes.borrow_mut().push((id, node));
                Ok(Box::new(TestPage {
                    id,
                    node,
                    dropped: dropped.clone(),
                }) as Box<dyn NavPage>)
            }
        };
        let host = TabHost::new(stacks.clone(), factory)
            .unwrap()
            .with_policy(PagePolicy::Transient);
        (
            host,
            stacks,
            Harness {
                built,
                dropped,
                nodes,
            },
        )
    }

    fn tick(host: &mut TabHost<u8, u8>) {
        host.on_event(&Event::CursorEntered);
    }

    fn node_of(h: &Harness, id: PageId) -> NodeId {
        h.nodes
            .borrow()
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, n)| *n)
            .unwrap()
    }

    #[test]
    fn builds_only_the_tab_you_visit() {
        let (mut host, stacks, h) = build();
        assert_eq!(
            *h.built.borrow(),
            vec![(0, 0)],
            "only the active tab's root is built at construction"
        );

        stacks.select(2);
        assert!(host.sync());
        assert_eq!(*h.built.borrow(), vec![(0, 0), (2, 2)]);
        assert_eq!(host.current_tab(), 2);
        assert!(
            h.dropped.borrow().is_empty(),
            "leaving a tab tears nothing down"
        );

        assert_eq!(
            h.built.borrow().len(),
            2,
            "tab 1 was never visited, so it was never built"
        );
    }

    /// The whole reason nested stacks exist: a tab keeps its own depth while you are away from it.
    #[test]
    fn each_tab_keeps_its_own_stack_across_a_switch() {
        let (mut host, stacks, h) = build();
        stacks.push(10);
        tick(&mut host);
        stacks.push(11);
        tick(&mut host);
        assert_eq!(host.current_route(), Some(11));
        assert_eq!(stacks.depth(), 3);

        stacks.select(1);
        tick(&mut host);
        assert_eq!(host.current_tab(), 1);
        assert_eq!(
            stacks.depth(),
            1,
            "the tab you arrive at is at its own root, not the depth you left behind"
        );
        assert_eq!(host.current_route(), Some(1));

        stacks.select(0);
        tick(&mut host);
        assert_eq!(
            host.current_route(),
            Some(11),
            "coming back lands three screens deep, where you left it"
        );
        assert_eq!(stacks.depth(), 3);
        assert!(
            !h.dropped.borrow().contains(&(0, 11)),
            "the deep screen was never released: {:?}",
            h.dropped.borrow()
        );
        assert_eq!(
            *h.built.borrow(),
            vec![(0, 0), (0, 10), (0, 11), (1, 1)],
            "nothing was rebuilt on the way back"
        );
    }

    /// With tab history on, Back walks out of the active tab once its stack is at its root — the Android behaviour — and only reports "nothing to do" when there is no tab left to return to either.
    #[test]
    fn tab_history_lets_back_walk_out_to_the_previous_tab() {
        reset_layout_runtime();
        let stacks =
            TabStacks::new(signal(0u8), &[0, 1, 2], |tab| Navigator::new(*tab)).with_tab_history();
        let mut host = TabHost::new(stacks.clone(), |_: &u8, route: &u8| {
            let (node, _rect) = new_leaf(LayoutStyle::new())?;
            Ok(Box::new(TestPage {
                id: (0, *route),
                node,
                dropped: Rc::new(RefCell::new(Vec::new())),
            }) as Box<dyn NavPage>)
        })
        .unwrap();

        stacks.select(1);
        tick(&mut host);
        stacks.select(2);
        tick(&mut host);
        stacks.push(20);
        tick(&mut host);
        assert!(stacks.can_pop());

        assert!(stacks.back());
        tick(&mut host);
        assert_eq!(host.current_tab(), 2);

        assert!(stacks.back());
        tick(&mut host);
        assert_eq!(host.current_tab(), 1);
        assert!(stacks.back());
        tick(&mut host);
        assert_eq!(host.current_tab(), 0);
        assert!(
            !stacks.back(),
            "nothing left to go back to, so the OS gesture can have it"
        );
        assert!(!stacks.can_pop());
    }

    /// Back never bounces: returning to a tab must not record the one being left, or two tabs would trade places forever instead of the history walking out.
    #[test]
    fn walking_back_through_tabs_does_not_record_the_tab_it_leaves() {
        reset_layout_runtime();
        let stacks =
            TabStacks::new(signal(0u8), &[0, 1], |tab| Navigator::new(*tab)).with_tab_history();
        stacks.select(1);
        assert!(stacks.back());
        assert_eq!(stacks.peek_active(), 0);
        assert!(
            !stacks.back(),
            "the history is empty again, not holding tab 1"
        );
    }

    /// Without tab history (the default), Back is scoped to the active tab — it pops that stack rather than walking to the tab you came from, which is the iOS tab-bar model.
    #[test]
    fn back_pops_the_active_tab_and_never_switches_tabs() {
        let (mut host, stacks, _h) = build();
        stacks.push(10);
        tick(&mut host);
        stacks.select(1);
        tick(&mut host);

        assert!(
            !stacks.back(),
            "tab 1 is at its root, so there is nothing to go back to — the OS gesture can have it"
        );
        assert_eq!(host.current_tab(), 1, "back did not return to tab 0");

        stacks.select(0);
        tick(&mut host);
        assert!(stacks.back(), "tab 0 still has its pushed screen to pop");
        tick(&mut host);
        assert_eq!(host.current_route(), Some(0));
    }

    /// Selecting the tab you are already on is the platform's "go home": it pops that tab to its root instead of doing nothing. It is also what replaces the per-route `KeepAlive` dance a single shared stack needed.
    #[test]
    fn reselecting_the_active_tab_pops_it_to_its_root() {
        let (mut host, stacks, h) = build();
        stacks.push(10);
        tick(&mut host);
        stacks.push(11);
        tick(&mut host);

        stacks.select(0);
        tick(&mut host);
        assert_eq!(host.current_route(), Some(0));
        assert_eq!(stacks.depth(), 1);
        let mut gone = h.dropped.borrow().clone();
        gone.sort();
        assert_eq!(
            gone,
            vec![(0, 10), (0, 11)],
            "the popped screens were released"
        );
    }

    #[test]
    fn displays_only_the_active_tab() {
        let (mut host, stacks, h) = build();
        stacks.select(1);
        tick(&mut host);
        compute_layout(
            host.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .ok();
        let active = absolute_rect(node_of(&h, (1, 1)))
            .map(|r| r.width)
            .unwrap_or(0.0);
        let hidden = absolute_rect(node_of(&h, (0, 0)))
            .map(|r| r.width)
            .unwrap_or(0.0);
        assert!(active > 0.0, "the active tab fills the host");
        assert_eq!(hidden, 0.0, "the tab you left is collapsed out of layout");
    }

    /// A stack pushed while its tab was off screen is reconciled when the tab comes back in, so a deep link or a redirect into a background tab lands on the right screen rather than the one it left.
    #[test]
    fn a_background_tabs_stack_is_reconciled_on_the_way_in() {
        let (mut host, stacks, _h) = build();
        stacks.select(1);
        tick(&mut host);
        stacks.select(0);
        stacks.navigator_for(&1).unwrap().push(42);
        tick(&mut host);
        stacks.select(1);
        tick(&mut host);
        assert_eq!(host.current_route(), Some(42));
    }

    #[test]
    fn an_active_tab_outside_the_set_falls_back_to_the_first() {
        reset_layout_runtime();
        let active = signal(9u8);
        let stacks = TabStacks::new(active, &[0, 1], |tab| Navigator::new(*tab));
        assert_eq!(active.peek(), 0);
        assert_eq!(stacks.peek_active(), 0);
        assert!(stacks.navigator_for(&9).is_none());
    }

    #[test]
    fn tabs_are_reported_in_declaration_order() {
        reset_layout_runtime();
        let stacks = TabStacks::new(signal(1u8), &[2, 1, 0], |tab| Navigator::new(*tab));
        assert_eq!(stacks.tabs().copied().collect::<Vec<_>>(), vec![2, 1, 0]);
        assert_eq!(
            stacks.peek_active(),
            1,
            "an active tab already in the set is left alone"
        );
    }
}
