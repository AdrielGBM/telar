use layout_core::NodeId;
use ui_tree::Component;

use crate::layout_leaf::LayoutLeaf;

pub trait LeafWidget {
    fn layout_leaf(&self) -> &LayoutLeaf;
}

pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;
}

impl<T: LeafWidget + Component> LayoutItem for T {
    fn layout_node(&self) -> NodeId {
        self.layout_leaf().node
    }
}

/// Implements `LeafWidget` for a struct that has a `leaf: LayoutLeaf` field.
#[macro_export]
macro_rules! impl_leaf_widget {
    ($struct:ident) => {
        impl $crate::layout_item::LeafWidget for $struct {
            fn layout_leaf(&self) -> &$crate::layout_leaf::LayoutLeaf {
                &self.leaf
            }
        }
    };
}
