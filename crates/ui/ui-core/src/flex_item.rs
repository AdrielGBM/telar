use layout_core::NodeId;

use ui_tree::Component;

pub trait FlexItem: Component {
    fn layout_node(&self) -> NodeId;
}
