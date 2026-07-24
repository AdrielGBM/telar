use platform_core::Event;
use ui_core::{Component, EventResult, LayoutItem, NodeId, RenderNode};

/// A single screen managed by a [`NavHost`](crate::NavHost).
///
/// A page is a self-contained view: a [`LayoutItem`] (so it renders, handles events, and owns a layout node)
/// plus two lifecycle hooks the host calls when navigation makes it the active page. Both default to no-ops,
/// so a screen with no enter/relayout behavior can be wrapped with [`SimplePage`] instead of implementing the
/// trait by hand.
pub trait NavPage: LayoutItem {
    /// Called when this page becomes the active top of the stack, after its layout node is shown. Autofocus
    /// the primary input here.
    fn on_enter(&mut self) {}

    /// Called when the host re-lays-out while this page is active — re-lay this page's own scroll viewport(s)
    /// against their now-known size (the host's layout pass does not reach those separate roots).
    fn on_relayout(&mut self) {}
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
