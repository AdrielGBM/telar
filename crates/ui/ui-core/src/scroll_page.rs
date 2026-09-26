//! A full-window scrolling page, and the surface's primary scroll.

use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId, SizeDimension};
use platform_core::{Event, PrimaryScrollClaim};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty, new_container};
use crate::layout_item::LayoutItem;
use crate::scroll_area::{LayoutScrollArea, ScrollViewport};

/// A window-sized root holding a [`LayoutScrollArea`] whose viewport is recomputed on resize, so content scrolls against the current window dimensions.
///
/// Its scroll is the surface's **primary scroll**, the one that stands for the whole page. Each target keeps it in its own way:
///
/// - web-dom: the document's own scroll. The page grows with the content, the browser scrolls it and shows its own bar, and the scroll events the window reports become this page's offset.
/// - web canvas, desktop, Android, TUI and headless: a scroll area like any other, drawn at the offset inside a surface the size of the window.
///
/// While it is alive a platform can read and move it through [`platform_core::primary_scroll_offset`] and [`platform_core::scroll_primary_to`], which is how a location adapter keeps a scroll position per history entry.
pub struct ScrollPage {
    root: NodeId,
    content_node: NodeId,
    scroll_area: LayoutScrollArea,
    viewport: ScrollViewport,
    _primary: PrimaryScrollClaim,
}

impl ScrollPage {
    pub fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        Self::new_with(move |_| Ok(content))
    }

    /// Like [`new`](Self::new), but the content is built with the page's [`ScrollViewport`], for content that reveals its own parts or scrolls the page itself.
    pub fn new_with<F>(build: F) -> Result<Self, LayoutError>
    where
        F: FnOnce(ScrollViewport) -> Result<Box<dyn LayoutItem>, LayoutError>,
    {
        let mut viewport = None;
        let scroll_area = LayoutScrollArea::new_with(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            |handed| {
                viewport = Some(handed.clone());
                build(handed)
            },
        )?
        .into_primary();
        let viewport = viewport.expect("the scroll area hands its viewport to the builder");
        let content_node = scroll_area.content_node();
        // Percent sizing, so a `compute_layout` against `Definite(w, h)` gives the leaf a full-window viewport.
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[scroll_area.layout_node()],
        )?;
        let primary = platform_core::claim_primary_scroll(Rc::new(viewport.clone()));
        Ok(Self {
            root,
            content_node,
            scroll_area,
            viewport,
            _primary: primary,
        })
    }

    /// The page's viewport: its offset, its visible rect, and the calls that scroll it.
    pub fn viewport(&self) -> ScrollViewport {
        self.viewport.clone()
    }

    /// Lays the page out against a window of `width`×`height`, then the content against that width at its own natural height — which is what gives the scroll area something taller than itself to scroll.
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

#[cfg(test)]
#[path = "scroll_page_test.rs"]
mod tests;
