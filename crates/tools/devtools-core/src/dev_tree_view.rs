use geometry_core::Rect;
use ui_tree::{ComponentList, SegmentNodeInfo};

pub struct DevNodeInfo {
    pub id: u64,
    pub name: &'static str,
    pub rect: Rect,
    pub depth: usize,
}

pub trait DevTreeView {
    fn node_count(&self) -> usize;
    fn for_each_node(&self, f: &mut dyn FnMut(&DevNodeInfo));
}

impl DevTreeView for ComponentList {
    fn node_count(&self) -> usize {
        let mut nodes = Vec::new();
        self.walk_tree(&mut nodes);
        nodes.len()
    }

    fn for_each_node(&self, f: &mut dyn FnMut(&DevNodeInfo)) {
        let mut nodes: Vec<SegmentNodeInfo> = Vec::new();
        self.walk_tree(&mut nodes);
        for node in &nodes {
            f(&DevNodeInfo {
                id: node.id,
                name: node.name,
                rect: node.rect,
                depth: node.depth,
            });
        }
    }
}
