use layout_core::NodeId;
use ui_tree::Component;

pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;
}
