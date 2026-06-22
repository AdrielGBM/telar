use geometry_core::Rect as Bounds;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{WidgetCtx, new_container, track_layout};
use crate::layout_item::LayoutItem;
use crate::pointer::dispatch_container_event;

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Bounds>,
    style: Box<dyn Fn(Bounds) -> RectStyle>,
    opacity: f32,
    children: Vec<(Box<dyn LayoutItem>, Option<RwSignal<Bounds>>)>,
}

impl StyledContainer {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        style: impl Fn(Bounds) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
        let node = new_container(ctx, layout_style, &child_nodes)?;
        let rect = track_layout(ctx, node).expect("new_container always registers a signal");
        let children = children
            .into_iter()
            .map(|c| {
                let rect = track_layout(ctx, c.layout_node());
                (c, rect)
            })
            .collect();
        Ok(Self {
            node,
            rect,
            style: Box::new(style),
            opacity: 1.0,
            children,
        })
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl LayoutItem for StyledContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for StyledContainer {
    fn view(&self) -> RenderNode {
        let r = self.rect.get();
        let bg = RenderNode::rect(
            Bounds {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            (self.style)(r),
        );
        let content = RenderNode::group(
            std::iter::once(bg).chain(self.children.iter().map(|(c, _)| c.view())),
        );
        if self.opacity < 1.0 {
            RenderNode::layer(self.opacity, 0.0, [content])
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.children, event)
    }
}
