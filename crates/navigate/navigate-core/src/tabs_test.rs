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
    assert!(host.sync(), "the first sync builds the tab you land on");
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
    assert!(stacks.can_pop(), "the stack has somewhere to go back to");

    assert!(stacks.back(), "back walks within the tab");
    tick(&mut host);
    assert_eq!(host.current_tab(), 2);

    assert!(stacks.back(), "and again");
    tick(&mut host);
    assert_eq!(host.current_tab(), 1);
    assert!(stacks.back(), "and out to the previous tab");
    tick(&mut host);
    assert_eq!(host.current_tab(), 0);
    assert!(
        !stacks.back(),
        "nothing left to go back to, so the OS gesture can have it"
    );
    assert!(
        !stacks.can_pop(),
        "the first page of the first tab is the end of the line"
    );
}

/// Back never bounces: returning to a tab must not record the one being left, or two tabs would trade places forever instead of the history walking out.
#[test]
fn walking_back_through_tabs_does_not_record_the_tab_it_leaves() {
    reset_layout_runtime();
    let stacks =
        TabStacks::new(signal(0u8), &[0, 1], |tab| Navigator::new(*tab)).with_tab_history();
    stacks.select(1);
    assert!(
        stacks.back(),
        "walking back leaves the tab without recording it"
    );
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
    assert!(
        stacks.navigator_for(&9).is_none(),
        "there is no tab 9 to navigate"
    );
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
