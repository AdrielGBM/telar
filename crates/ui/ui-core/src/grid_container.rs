use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::context::new_container;
use crate::layout_item::LayoutItem;

pub struct GridContainer {
    node: NodeId,
    children: Vec<Box<dyn LayoutItem>>,
}

impl GridContainer {
    pub fn new(
        style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
        let node = new_container(style, &child_nodes)?;
        Ok(GridContainer { node, children })
    }
}

impl LayoutItem for GridContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for GridContainer {
    fn view(&self) -> View {
        let child_views: Vec<View> = self.children.iter().map(|c| c.view()).collect();
        View::group(child_views)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        for child in &mut self.children {
            if child.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
}
