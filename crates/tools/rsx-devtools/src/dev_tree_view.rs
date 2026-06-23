use geometry_core::Rect;
use renderer_core::DrawCommand;
use ui_tree::ComponentList;

pub struct DevNodeInfo {
    pub id: u64,
    pub rect: Rect,
    pub depth: usize,
}

pub trait DevTreeView {
    fn node_count(&self) -> usize;
    fn for_each_node(&self, f: &mut dyn FnMut(&DevNodeInfo));
}

impl DevTreeView for ComponentList {
    fn node_count(&self) -> usize {
        self.commands().len()
    }

    fn for_each_node(&self, f: &mut dyn FnMut(&DevNodeInfo)) {
        let cmds = self.commands();
        for (idx, cmd) in cmds.iter().enumerate() {
            let rect = match cmd {
                DrawCommand::Rect { rect, .. } => *rect,
                DrawCommand::Text { rect, .. } => *rect,
                DrawCommand::Image { rect, .. } => *rect,
                DrawCommand::PushClip { rect, .. } => *rect,
                _ => Rect::default(),
            };
            f(&DevNodeInfo {
                id: idx as u64,
                rect,
                depth: 0,
            });
        }
    }
}
