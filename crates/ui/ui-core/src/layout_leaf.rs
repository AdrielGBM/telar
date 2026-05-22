use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::ReadSignal;

use crate::context;

pub struct LayoutLeaf {
    pub node: NodeId,
    pub rect: ReadSignal<Rect>,
}

impl LayoutLeaf {
    pub fn register(style: LayoutStyle) -> Result<Self, LayoutError> {
        let (node, rect) = context::register_leaf(style)?;
        Ok(Self { node, rect })
    }
}
