use telar::{
    AccessNode, App, Component, ComponentList, DrawCommand, Rect, Role, WindowRoot,
    for_each_with_matrix, relayout_if_dirty, transform_clip_rect,
};
use telar_devtools::workbench_scope;

use crate::state::WorkshopState;

const SETTLE_ROUNDS: usize = 4;

pub struct Shell {
    pub state: WorkshopState,
}

impl App for Shell {
    fn root(&self) -> Box<dyn Component> {
        let chrome =
            workbench_scope(|| crate::shell::shell(&self.state, None)).expect("the chrome builds");
        Box::new(WindowRoot::wrapping(Box::new(chrome)).expect("the window builds"))
    }
}

/// A string as the window shows it: where it lands, and the clip it is drawn under.
pub struct Drawn {
    pub text: String,
    pub rect: Rect,
    pub clip: Option<Rect>,
}

pub fn drawn(tree: &ComponentList) -> Vec<Drawn> {
    let mut clips = Vec::new();
    let mut found = Vec::new();
    for_each_with_matrix(&tree.commands(), |command, matrix| match command {
        DrawCommand::PushClip { rect, .. } => clips.push(transform_clip_rect(matrix, *rect)),
        DrawCommand::PopClip => {
            clips.pop();
        }
        DrawCommand::Text { text, rect, .. } => found.push(Drawn {
            text: text.to_string(),
            rect: transform_clip_rect(matrix, *rect),
            clip: clips.last().copied(),
        }),
        _ => {}
    });
    found
}

pub fn settle(tree: &ComponentList) {
    for _ in 0..SETTLE_ROUNDS {
        relayout_if_dirty();
        let _ = tree.commands();
    }
}

/// The control of `role` that `matches`, among the nodes that take part in the interaction.
pub fn control_matching(
    nodes: &[AccessNode],
    role: Role,
    label: &str,
    matches: impl Fn(&AccessNode) -> bool,
) -> AccessNode {
    nodes
        .iter()
        .find(|node| node.id.is_some() && node.role == role && matches(node))
        .cloned()
        .unwrap_or_else(|| {
            let controls: Vec<_> = nodes
                .iter()
                .filter(|node| node.id.is_some())
                .map(|node| (node.role, node.name.as_str()))
                .collect();
            panic!("no {role:?} named {label:?} among {controls:?}")
        })
}

pub fn control(nodes: &[AccessNode], role: Role, name: &str) -> AccessNode {
    control_matching(nodes, role, name, |node| node.name == name)
}
