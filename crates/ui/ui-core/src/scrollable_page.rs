use geometry_core::Rect;
use layout_core::AvailableSpace;
use platform_core::Event;
use reactive_core::{RwSignal, create_rw_signal};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{WidgetCtx, compute_layout};
use crate::layout_item::LayoutItem;
use crate::scroll_area::ScrollArea;

/// A ready-made scrollable full-window page that handles resize events, scroll clamping, and layout
/// recomputation automatically. Replaces the manual wiring of ScrollArea + compute_layout +
/// WindowResized handling that every scrollable app root would otherwise duplicate.
pub struct ScrollablePage {
    ctx: WidgetCtx,
    content_node: layout_core::NodeId,
    window_width: RwSignal<f32>,
    window_height: RwSignal<f32>,
    scroll_area: ScrollArea,
}

impl ScrollablePage {
    /// Create a new `ScrollablePage`. The `content` is the widget tree to display inside the
    /// scrollable viewport. `window_width` and `window_height` are closures returning the current
    /// window dimensions; they are called reactively on each view.
    pub fn new(
        ctx: WidgetCtx,
        content: Box<dyn LayoutItem>,
        initial_width: f32,
        initial_height: f32,
    ) -> Self {
        let window_width = create_rw_signal(initial_width);
        let window_height = create_rw_signal(initial_height);
        let content_node = content.layout_node();
        let ww = window_width.clone();
        let wh = window_height.clone();
        let scroll_area = ScrollArea::new(
            &ctx,
            move || Rect::new(0.0, 0.0, ww.get(), wh.get()),
            content,
        );
        // ctx is moved here after ScrollArea::new borrows it immutably above (the borrow ends before this line).
        Self {
            ctx,
            content_node,
            window_width,
            window_height,
            scroll_area,
        }
    }

    /// Recompute layout for the content node given the current window width. Call this after
    /// construction to perform the initial layout pass.
    pub fn compute_layout(&mut self) {
        let w = self.window_width.get();
        compute_layout(
            &mut self.ctx,
            self.content_node,
            AvailableSpace::Definite(w),
            AvailableSpace::MaxContent,
        )
        .ok();
    }

    /// Clamp scroll offsets so they don't exceed the content bounds. Call after resizing.
    pub fn clamp_scroll(&mut self) {
        self.scroll_area.clamp_scroll();
    }
}

impl Component for ScrollablePage {
    fn view(&self) -> RenderNode {
        self.scroll_area.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            let w = *width as f32;
            self.window_width.set(w);
            self.window_height.set(*height as f32);
            self.ctx.mark_dirty_node(self.content_node).ok();
            compute_layout(
                &mut self.ctx,
                self.content_node,
                AvailableSpace::Definite(w),
                AvailableSpace::MaxContent,
            )
            .ok();
            self.scroll_area.clamp_scroll();
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}
