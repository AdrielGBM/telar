//! What a page is to the host: a widget with lifecycle hooks and a policy for how long it is kept.

use platform_core::Event;
use ui_core::{Component, EventResult, LayoutItem, NodeId, RenderNode};

/// A single screen managed by a [`NavHost`](crate::NavHost).
///
/// A page is a self-contained view: a [`LayoutItem`] (so it renders, handles events, and owns a layout node) plus two lifecycle hooks the host calls when navigation makes it the active page. Both default to no-ops, so a screen with no enter/relayout behavior can be wrapped with [`SimplePage`] instead of implementing the trait by hand.
pub trait NavPage: LayoutItem {
    /// Called when this page becomes the active top of the stack, after its layout node is shown. Autofocus the primary input here.
    fn on_enter(&mut self) {}

    /// Called when the host re-lays-out while this page is active — re-lay this page's own scroll viewport(s) against their now-known size (the host's layout pass does not reach those separate roots).
    fn on_relayout(&mut self) {}
}

/// What a page's identity and lifetime are tied to — declared per destination via [`NavHost::set_policy_for`](crate::NavHost::set_policy_for), because one host commonly serves both kinds: a fixed set of persistent destinations (a rail, a tab bar) plus screens pushed as a stack on top of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PagePolicy {
    /// Identity is the **route**: one page per route for the life of the host. Revisiting reuses that subtree and everything in it — scroll position, form state, in-flight work — at the cost of never releasing a screen the user left, and of a route that appears twice on the stack sharing one page between both positions. Right for a small fixed set of destinations, wrong for an unbounded stack.
    #[default]
    KeepAlive,
    /// Identity is the **stack entry**: pushing builds a page, popping releases it, and the same route pushed at two depths gets two independent pages — the semantics of a native page stack, which pushes instances rather than routes. Going back still finds the screen you left as you left it, because its entry never went away. Releasing the page drops its widgets, and with them their signals and effects, which is the framework's only cascading teardown.
    Transient,
}

/// Wraps a plain [`LayoutItem`] as a hook-less [`NavPage`], for screens that need no enter/relayout behavior.
pub struct SimplePage(pub Box<dyn LayoutItem>);

impl SimplePage {
    pub fn new(item: impl LayoutItem + 'static) -> Self {
        Self(Box::new(item))
    }
}

impl Component for SimplePage {
    fn view(&self) -> RenderNode {
        self.0.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.on_event(event)
    }
}

impl LayoutItem for SimplePage {
    fn layout_node(&self) -> NodeId {
        self.0.layout_node()
    }
}

impl NavPage for SimplePage {}
