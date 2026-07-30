use crate::{
    App, AppConfig, AvailableSpace, Color, Component, Container, Event, EventResult, LayoutError,
    LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, PreviewEntry, RenderNode, SizeDimension,
    Text, TextStyle, compute_layout, mark_dirty, new_container, reset_layout_runtime,
};

pub struct PreviewApp {
    pub entries: Vec<PreviewEntry>,
}

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
        // Root fills the window via percent sizing so compute_layout against Definite(w, h) yields a full-window viewport for the scroll-area leaf.
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

    fn recompute_layout(&mut self, width: f32, height: f32) {
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
            self.recompute_layout(*width as f32, *height as f32);
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

impl App for PreviewApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();

        // `cargo telar preview --component <name>` sets this to scope the window to one component.
        let wanted = std::env::var("TELAR_PREVIEW_COMPONENT")
            .ok()
            .filter(|s| !s.is_empty());

        for entry in self
            .entries
            .iter()
            .filter(|e| wanted.as_deref().is_none_or(|w| e.component_name == w))
        {
            let header_text = format!("[{}]  {}", entry.component_name, entry.preview_name);
            let header = Text::new(
                move || header_text.clone(),
                LayoutStyle::new().padding_all(8.0),
                || TextStyle::new(11.0, Color::rgba(0.4, 0.4, 0.55, 1.0)),
            )
            .unwrap();

            let mut children: Vec<Box<dyn LayoutItem>> = vec![Box::new(header)];
            match (entry.build)() {
                Ok(widget) => children.push(widget),
                Err(err) => {
                    let msg = format!("Error: {err}");
                    let label = Text::new(
                        move || msg.clone(),
                        LayoutStyle::new(),
                        || TextStyle::new(12.0, Color::rgba(0.9, 0.2, 0.2, 1.0)),
                    )
                    .unwrap();
                    children.push(Box::new(label));
                }
            }

            let section = Container::new(
                LayoutStyle::new().flex_column().gap(8.0).padding_all(16.0),
                children,
            )
            .unwrap();
            sections.push(Box::new(section));
        }

        let content = Container::new(
            LayoutStyle::new().flex_column().gap(16.0).padding_all(24.0),
            sections,
        )
        .unwrap();

        let page = ScrollPage::new(Box::new(content)).expect("page layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgba(0.96, 0.96, 0.98, 1.0))
    }
}

pub fn run_preview_window(entries: Vec<PreviewEntry>, config: AppConfig) {
    crate::run_app_with_name(config, PreviewApp { entries }, "telar-preview");
}
