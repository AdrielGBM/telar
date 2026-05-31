use layout_core::NodeId;
use ui_tree::Component;

use crate::layout_leaf::LayoutLeaf;

pub trait HasLayoutLeaf {
    fn layout_leaf(&self) -> &LayoutLeaf;
}

pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;
}

impl<T: HasLayoutLeaf + Component> LayoutItem for T {
    fn layout_node(&self) -> NodeId {
        self.layout_leaf().node
    }
}
