//! [`ScopeProvider`]: a subtree drawn in a theme or at a control size of its own.

use layout_core::{LayoutError, NodeId};
use platform_core::Event;
use theme_core::{ControlSize, ScopedControlSize, ScopedTheme};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::record_nodes;
use crate::layout_item::{Child, LayoutItem, build_owned, make_child};

/// Builds `child` with `theme` in force for it and for everything beneath it. `theme` is either a theme value held for the subtree's lifetime, or a [`ScopedTheme`] the caller keeps to switch the subtree later; switching it re-runs only readers inside the subtree, and the global [`set_theme`](theme_core::set_theme) no longer reaches them.
pub fn provide_theme(
    theme: impl Into<ScopedTheme>,
    child: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<ScopeProvider, LayoutError> {
    ScopeProvider::build("ThemeProvider", child, move |built| {
        let theme = theme.into();
        theme.provide();
        let built = built()?;
        crate::inherit::bind_theme(built.layout_node(), theme);
        Ok(built)
    })
}

/// A scoped theme that follows what `read` reads: the theme `theme:` with a value reading `$state` provides, re-read when that state changes, so switching it re-runs only the readers under it.
pub fn follow_theme<T: theme_core::ThemeTokens + Clone + 'static>(
    read: impl Fn() -> T + 'static,
) -> ScopedTheme {
    let scoped = ScopedTheme::new(read());
    reactive_core::effect(move || scoped.set(read()));
    scoped
}

/// Builds `child` with `size` as the control size of it and everything beneath it, over the surface's and the thread's. `size` is a [`ControlSize`] held for the subtree's lifetime, or a [`ScopedControlSize`] the caller keeps to switch it later; switching it re-spaces only the controls inside the subtree.
pub fn provide_control_size(
    size: impl Into<ScopedControlSize>,
    child: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<ScopeProvider, LayoutError> {
    ScopeProvider::build("ControlSizeProvider", child, move |built| {
        size.into().provide();
        built()
    })
}

/// A scoped control size that follows what `read` reads, so a subtree's density can be bound to state.
pub fn follow_control_size(read: impl Fn() -> ControlSize + 'static) -> ScopedControlSize {
    let scoped = ScopedControlSize::new(read());
    reactive_core::effect(move || scoped.set(read()));
    scoped
}

/// What [`provide_theme`] and [`provide_control_size`] return. Adds no layout node: it lays out, draws and takes events exactly as its child does, under the owner the scope was provided to.
pub struct ScopeProvider {
    child: Child,
    name: &'static str,
}

impl ScopeProvider {
    /// Runs `scope` under a fresh owner, handing it the child's build to run once it has provided what the subtree reads.
    fn build<C: FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>>(
        name: &'static str,
        child: C,
        scope: impl FnOnce(C) -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Self, LayoutError> {
        let nodes = record_nodes();
        // Mounted here rather than by the parent, so the child's rendering re-runs under the scope it was provided to.
        let (child, _) = build_owned(|| scope(child).map(make_child))?;
        nodes.keep();
        Ok(Self { child, name })
    }
}

impl LayoutItem for ScopeProvider {
    fn layout_node(&self) -> NodeId {
        self.child.node()
    }

    fn occludes(&self) -> bool {
        self.child
            .item
            .try_borrow()
            .map(|item| item.occludes())
            .unwrap_or(true)
    }
}

impl Component for ScopeProvider {
    fn view(&self) -> RenderNode {
        self.child.boundary()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.child.deliver(event)
    }

    fn debug_name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
#[path = "theme_provider_test.rs"]
mod tests;
