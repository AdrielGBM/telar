use geometry_core::Rect;

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
