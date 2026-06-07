use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::RwSignal;
use ui_tree::RenderNode;

use crate::context;

pub struct LayoutLeaf {
    pub node: NodeId,
    pub rect: RwSignal<Rect>,
}

impl LayoutLeaf {
    pub fn register(ctx: &mut context::WidgetCtx, style: LayoutStyle) -> Result<Self, LayoutError> {
        let (node, rect) = context::new_leaf(ctx, style)?;
        Ok(Self { node, rect })
    }

    pub(crate) fn at_layout_position(&self, content: RenderNode) -> RenderNode {
        let r = self.rect.get();
        RenderNode::Transform {
            matrix: [1.0, 0.0, 0.0, 1.0, r.x, r.y],
            children: vec![content],
        }
    }
}
