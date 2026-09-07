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
    let mut host = NavHost::new(nav.clone(), factory).unwrap();
    host.set_transition(transition);
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
    assert_eq!(host.built().len(), 2, "the popped page is still cached");
    nav.push(1);
    tick(&mut host);
    assert_eq!(
        *h.built.borrow(),
        vec![0, 1],
        "revisiting reuses the cached page rather than rebuilding it"
    );
}

/// `KeepAlive` files a page under its route, so a stack naming the same route twice shares one page — one scroll position, one set of widget state — between both positions. That is what a persistent destination means, and the opposite of what a page stack does (see the `Transient` case below).
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
    assert_eq!(host.built().len(), 2);
    assert_eq!(node_of(&h, 0), host.built()[0].node);
}

/// `Transient` files a page under its stack entry, which is what makes it a real page stack: the same route pushed at two depths is two independent screens, and popping one releases only that one.
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
    assert_eq!(host.built().len(), 3);
    assert!(dropped.borrow().is_empty(), "nothing has been dropped yet");

    nav.pop();
    tick(&mut host);
    assert_eq!(
        *dropped.borrow(),
        vec![0],
        "only the popped entry's page was released"
    );
    assert_eq!(
        host.built().len(),
        2,
        "the root's own page for route 0 is untouched"
    );
    assert_eq!(host.current(), 1);
}

/// A `replace` reuses the top stack slot for a different route, so the entry-keyed page filed under that slot must be released rather than left behind for a screen the user can no longer reach.
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

/// The mixed host the policy-per-destination API exists for: rail destinations kept as the user left them, with a detail pushed over them that is fresh per push and released on the way back.
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

    nav.pop_to_root();
    tick(&mut host);
    let mut gone = dropped.borrow().clone();
    gone.sort();
    assert_eq!(gone, vec![1, 2], "only the popped pages were released");
    assert_eq!(host.built().len(), 1, "the root page is still built");
    assert_eq!(host.current(), 0);

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
        let (source, runs) = (source, runs.clone());
        move |route: &u8| {
            let (node, _rect) = new_leaf(LayoutStyle::new())?;
            // Only the pushed page owns an effect, so the counter tracks exactly that page's lifetime.
            let held = (*route == 1).then(|| {
                let (source, runs) = (source, runs.clone());
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

/// **A page that replaces another keeps its own effects**, whether or not a scope was open when the host was built.
///
/// The incoming page is built during the reconcile the outgoing one is torn down by, and a reconcile can be reached from inside the page being left — so a page parented to whichever owner happened to be running became a *child* of the page about to be pruned, and `dispose_owner` uproots descendants. Its layout node survived, so it drew; its effects, signals and contexts were already gone. The window went on composing and painting at full rate, and its handlers went on reporting events as handled, while nothing they set was read by anybody. Nothing logs and nothing panics: the fault is visible only as a window that ignores its user.
///
/// Run both ways because the two differ in what the host can capture, and the fix that serves one does not serve the other: an app root is mounted outside every scope, so a host that read the ambient owner there held `None` — and building under `None` pushes no frame at all, leaving the outgoing page current after all. Only the nested case would have passed.
///
/// Replacing rather than pushing is what makes the two pages share a slot, so the outgoing one is pruned in the same breath the incoming one is built — the narrowest form of the case.
#[test]
fn the_page_that_replaces_another_keeps_its_own_effects() {
    replacing_a_page_keeps_its_effects(Nesting::UnderAnOwner);
    replacing_a_page_keeps_its_effects(Nesting::UnderNone);
}

enum Nesting {
    UnderAnOwner,
    UnderNone,
}

fn replacing_a_page_keeps_its_effects(nesting: Nesting) {
    reset_layout_runtime();
    let source = signal(0i32);
    let runs = Rc::new(RefCell::new(0usize));
    let nav = Navigator::new(0u8);
    let factory = {
        let (source, runs) = (source, runs.clone());
        move |route: &u8| {
            let (node, _rect) = new_leaf(LayoutStyle::new())?;
            // Every page owns one, so what the counter tracks is whichever page is up.
            let held = {
                let (source, runs) = (source, runs.clone());
                let route = *route;
                Some(effect(move || {
                    source.get();
                    if route == 2 {
                        *runs.borrow_mut() += 1;
                    }
                }))
            };
            Ok(Box::new(EffectPage { node, _held: held }) as Box<dyn NavPage>)
        }
    };
    let mut host = match nesting {
        Nesting::UnderAnOwner => {
            let held = owner_scope();
            let host = NavHost::new(nav.clone(), factory).unwrap();
            drop(held);
            host
        }
        Nesting::UnderNone => NavHost::new(nav.clone(), factory).unwrap(),
    };
    host.set_policy_for(|_| PagePolicy::Transient);

    // Reconciled from inside the page being left, which is the situation this is about: a tick from a neutral owner does not reproduce it, since the incoming page is only parented to the outgoing one while it runs.
    let leaving = host.built()[0].owner;
    nav.replace(2);
    with_owner(Some(leaving), || tick(&mut host));

    let mounted = *runs.borrow();
    source.set(1);
    assert_eq!(
        *runs.borrow(),
        mounted + 1,
        "the page that arrived still answers a source it reads — it was not uprooted with the one it replaced"
    );
}
#[test]
fn sync_reconciles_without_an_event() {
    let (mut host, nav, h) = build(NavTransition::None);
    nav.push(1);
    host.sync();
    assert_eq!(*h.built.borrow(), vec![0, 1]);
    assert_eq!(host.current(), 1);

    h.log.borrow_mut().clear();
    host.sync();
    assert_eq!(*h.built.borrow(), vec![0, 1]);
    assert!(
        h.log.borrow().is_empty(),
        "sync reconciles without emitting an event"
    );
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
    assert!(
        matches!(host.view(), RenderNode::Empty),
        "nothing is built before the first sync"
    );

    nav.push(1);
    tick(&mut host);
    assert!(
        matches!(host.view(), RenderNode::Transform { .. }),
        "a slide transition wraps the page in a transform"
    );

    let (mut fade_host, fade_nav, _h2) = build(NavTransition::Fade);
    fade_nav.push(1);
    tick(&mut fade_host);
    assert!(
        matches!(fade_host.view(), RenderNode::Layer { .. }),
        "and a fade wraps it in a layer"
    );
}
