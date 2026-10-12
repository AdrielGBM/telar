use telar::preview::{PreviewCtx, PreviewEntry};
use telar::{
    AccessNode, App, Children, Component, ComponentList, DrawCommand, LayoutError, LayoutItem,
    Reactive, Rect, Role, WindowRoot, for_each_with_matrix, relayout_if_dirty, transform_clip_rect,
};
use telar_components::{BadgeProps, ButtonProps, badge, button};
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

/// The row of the preview settings menu that opens the submenu of the setting `name`, announced by its name and then the value it holds.
pub fn setting_row(nodes: &[AccessNode], name: &str) -> AccessNode {
    control_matching(nodes, Role::MenuItem, name, |node| {
        node.name == name || node.name.starts_with(&format!("{name} "))
    })
}

fn sample_button(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = ctx.arg("label", "Save".to_string());
    button(
        ButtonProps::props()
            .label(Reactive::of(move || label.clone()))
            .build(),
        Children::default(),
    )
}

fn sample_badge(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = ctx.arg("count", 3_i64);
    badge(
        BadgeProps::props()
            .label(Reactive::of(move || count.to_string()))
            .build(),
        Children::default(),
    )
}

/// A small library of previews written as an application writes them, over two groups, for what has to hold whatever the previews are.
pub fn sample_previews() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(
            "sample--button--primary",
            "button",
            "Primary",
            sample_button,
        )
        .title("Inputs/Button"),
        PreviewEntry::new("sample--button--ghost", "button", "Ghost", sample_button)
            .title("Inputs/Button"),
        PreviewEntry::new("sample--badge--count", "badge", "Count", sample_badge)
            .title("Display/Badge"),
    ]
}
