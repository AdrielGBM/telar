use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::RwSignal;

use crate::context;

pub struct LayoutLeaf {
    pub node: NodeId,
    pub rect: RwSignal<Rect>,
}

impl LayoutLeaf {
    pub fn register(ctx: &mut context::WidgetCtx, style: LayoutStyle) -> Result<Self, LayoutError> {
        let (node, rect) = context::register_leaf(ctx, style)?;
        Ok(Self { node, rect })
    }
}
