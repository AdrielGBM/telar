use crate::theme::theme;
use rsx::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, NodeId, RenderNode, SizeDimension, compute_layout, mark_dirty,
    new_container, reset_layout_runtime,
};

/// Full-window scrolling page: a window-sized root holding a LayoutScrollArea whose viewport is recomputed on resize so content scrolls against the current window dimensions.
struct ScrollPage {
    root: NodeId,
    content_node: NodeId,
    scroll_area: LayoutScrollArea,
}

impl ScrollPage {
    fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let scroll_area = LayoutScrollArea::new(
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            content,
        )?;
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

    fn relayout(&mut self, width: f32, height: f32) {
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

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            self.relayout(*width as f32, *height as f32);
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

pub struct LandingRoot;

impl App for LandingRoot {
    fn root(&self) -> Box<dyn rsx::Component> {
        reset_layout_runtime();
        let content = crate::home().expect("layout failed");
        let page = ScrollPage::new(content).expect("page layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}
