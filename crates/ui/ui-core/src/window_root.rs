//! The root every window's tree is mounted on.

use layout_core::AvailableSpace;
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty};
use crate::focus_mode::FocusMode;
use crate::layout_item::LayoutItem;

/// Lays its content out against the window: the root a runner mounts an application's tree on.
///
/// A percent-sized tree resolves to nothing until something hands it a definite space, so a runner never mounts what an application returns as it is. It mounts it on this, which lays the content's own node out at the window's size on every [`Event::WindowResized`]: as a layout root, content with an auto width or height fills the window on that axis, and a percent size is a fraction of it.
///
/// Not a [`LayoutItem`]: it is the top of a tree and never a child, so nothing nests one inside a box of another size, where it would still lay its content out at the window's.
pub struct WindowRoot {
    content: Box<dyn LayoutItem>,
}

impl WindowRoot {
    pub fn new(content: Box<dyn LayoutItem>) -> Self {
        Self { content }
    }

    fn lay_out(&self, width: u32, height: u32) {
        let root = self.content.layout_node();
        mark_dirty(root).ok();
        compute_layout(
            root,
            AvailableSpace::Definite(width as f32),
            AvailableSpace::Definite(height as f32),
        )
        .ok();
    }
}

impl Component for WindowRoot {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    /// Lays out first, then passes the resize on, so anything that has to run once the tree has real rects — a first-layout autofocus — sees it in that order.
    ///
    /// `Handled` regardless of what the content answered: the runner requests a redraw only for a handled event (`runner::handler`), so reporting the content's `Ignored` would relayout and never repaint.
    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            self.lay_out(*width, *height);
            self.content.on_event(event);
            return EventResult::Handled;
        }
        FocusMode::Standalone.dispatch(event, |event| self.content.on_event(event))
    }

    fn debug_name(&self) -> &'static str {
        "WindowRoot"
    }
}

#[cfg(test)]
#[path = "window_root.test.rs"]
mod tests;

#[cfg(test)]
#[path = "window_root.layout_cost.test.rs"]
mod layout_cost;
