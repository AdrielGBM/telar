use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::WidgetCtx;
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    opacity: f32,
    children: TrackedChildren,
}

impl StyledContainer {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(ctx, layout_style, children)?;
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
            Rect {
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
