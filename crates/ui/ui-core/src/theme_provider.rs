//! [`ThemeProvider`]: a subtree drawn in a theme of its own.

use layout_core::{LayoutError, NodeId};
use platform_core::Event;
use theme_core::ScopedTheme;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::record_nodes;
use crate::layout_item::{Child, LayoutItem, build_owned, make_child};

/// Builds `child` with `theme` in force for it and for everything beneath it. `theme` is either a theme value held for the subtree's lifetime, or a [`ScopedTheme`] the caller keeps to switch the subtree later; switching it re-runs only readers inside the subtree, and the global [`set_theme`](theme_core::set_theme) no longer reaches them.
pub fn provide_theme(
    theme: impl Into<ScopedTheme>,
    child: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<ThemeProvider, LayoutError> {
    let nodes = record_nodes();
    let (child, _) = build_owned(|| {
        let theme = theme.into();
        theme.provide();
        let built = child()?;
        crate::inherit::bind_theme(built.layout_node(), theme);
        // Mounted here rather than by the parent, so the child's rendering re-runs under the scope the theme was provided to.
        Ok(make_child(built))
    })?;
    nodes.keep();
    Ok(ThemeProvider { child })
}

/// What [`provide_theme`] returns. Adds no layout node: it lays out, draws and takes events exactly as its child does.
pub struct ThemeProvider {
    child: Child,
}

impl LayoutItem for ThemeProvider {
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

impl Component for ThemeProvider {
    fn view(&self) -> RenderNode {
        self.child.boundary()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.child.deliver(event)
    }

    fn debug_name(&self) -> &'static str {
        "ThemeProvider"
    }
}

#[cfg(test)]
#[path = "theme_provider_test.rs"]
mod tests;
