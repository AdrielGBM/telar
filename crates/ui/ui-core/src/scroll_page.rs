//! A full-window scrolling page.

use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId, SizeDimension};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty, new_container};
use crate::layout_item::LayoutItem;
use crate::scroll_area::LayoutScrollArea;

/// A window-sized root holding a [`LayoutScrollArea`] whose viewport is recomputed on resize, so content
/// scrolls against the current window dimensions.
///
/// The shape any app whose whole window is one scrolling column needs, and which two of them — the preview
/// runner and the landing page — had written out identically, down to the field names.
pub struct ScrollPage {
    root: NodeId,
    content_node: NodeId,
    scroll_area: LayoutScrollArea,
}

impl ScrollPage {
    pub fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let scroll_area = LayoutScrollArea::new(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            content,
        )?;
        // Percent sizing so a `compute_layout` against `Definite(w, h)` yields a full-window viewport for the
        // scroll-area leaf.
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[scroll_area.layout_node()],
        )?;
        Ok(Self {
            root,
            content_node,
            scroll_area,
        })
    }

    /// Lays the page out against a window of `width`×`height`, then the content against that width at its own
    /// natural height — which is what gives the scroll area something taller than itself to scroll.
    pub fn relayout(&mut self, width: f32, height: f32) {
        mark_dirty(self.root).ok();
        compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        )
        .ok();
        compute_layout(
            self.content_node,
            AvailableSpace::Definite(width),
            AvailableSpace::MaxContent,
        )
        .ok();
        self.scroll_area.clamp_scroll();
    }
}

impl Component for ScrollPage {
    fn view(&self) -> RenderNode {
        self.scroll_area.view()
    }

    /// A resize relays the page out; everything else goes to the scroll area.
    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            self.relayout(*width as f32, *height as f32);
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}
