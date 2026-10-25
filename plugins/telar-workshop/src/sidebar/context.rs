//! The sidebar's context menu: what a secondary press or the context-menu key offers on a preview, on a group, or on the empty space under the rows.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, ReactiveList, Rect, RwSignal, Slots,
    box_item,
};
use telar_components::{
    ContextMenuProps, ItemProps, SeparatorProps, TreeContext, TreeNode, context_menu, item,
    separator,
};

use super::model;
use crate::canvas::states::open_in_editor;
use crate::state::{ViewMode, WorkshopState};
use crate::strings::{
    self, COLLAPSE_ALL, COLLAPSE_GROUP, COPY_LINK, EXPAND_ALL, EXPAND_GROUP, OPEN_IN_EDITOR,
    SHOW_CANVAS, SHOW_DOCS, SHOW_MATRIX,
};

/// Where the menu was asked for, and the row it is about: `None` for the space under the rows.
#[derive(Clone)]
pub(super) struct Asked {
    pub(super) node: Option<TreeNode>,
    pub(super) at: (f32, f32),
    pub(super) around: Option<Rect>,
}

impl From<TreeContext> for Asked {
    fn from(context: TreeContext) -> Self {
        Self {
            node: Some(context.node),
            at: context.at,
            around: context.around,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Show(ViewMode),
    CopyLink,
    OpenInEditor,
    ExpandGroup,
    CollapseGroup,
    ExpandAll,
    CollapseAll,
}

impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::Show(ViewMode::Canvas) => SHOW_CANVAS,
            Self::Show(ViewMode::Matrix) => SHOW_MATRIX,
            Self::Show(ViewMode::Docs) => SHOW_DOCS,
            Self::CopyLink => COPY_LINK,
            Self::OpenInEditor => OPEN_IN_EDITOR,
            Self::ExpandGroup => EXPAND_GROUP,
            Self::CollapseGroup => COLLAPSE_GROUP,
            Self::ExpandAll => EXPAND_ALL,
            Self::CollapseAll => COLLAPSE_ALL,
        }
    }
}

/// The rows the menu shows for `node`, in order, `None` standing for a separator.
pub(super) fn commands(state: &WorkshopState, node: Option<&TreeNode>) -> Vec<Option<Command>> {
    let mut rows = match node {
        Some(group) if group.is_branch() => {
            vec![
                Some(Command::ExpandGroup),
                Some(Command::CollapseGroup),
                None,
            ]
        }
        Some(preview) => {
            let mut rows = vec![
                Some(Command::Show(ViewMode::Canvas)),
                Some(Command::Show(ViewMode::Matrix)),
                Some(Command::Show(ViewMode::Docs)),
                None,
                Some(Command::CopyLink),
            ];
            if written_in_a_known_file(state, &preview.id) {
                rows.push(Some(Command::OpenInEditor));
            }
            rows.push(None);
            rows
        }
        None => Vec::new(),
    };
    rows.extend([Some(Command::ExpandAll), Some(Command::CollapseAll)]);
    rows
}

fn written_in_a_known_file(state: &WorkshopState, id: &str) -> bool {
    state
        .entries()
        .iter()
        .any(|entry| entry.id == id && !entry.file.is_empty())
}

pub(super) fn run(state: &WorkshopState, command: Command, node: Option<&TreeNode>) {
    let id = node.map(|node| node.id.clone());
    match command {
        Command::Show(view) => {
            if id.is_some_and(|id| state.select(&id)) {
                state.view().set(view);
                state.sidebar_drawer().set_if_changed(false);
            }
        }
        Command::CopyLink => {
            if id.is_some_and(|id| state.select(&id)) {
                state.copy_link();
            }
        }
        Command::OpenInEditor => {
            let entry = id.and_then(|id| state.entries().iter().find(|entry| *entry.id == *id));
            if let Some(entry) = entry {
                open_in_editor(entry.file, entry.line);
            }
        }
        Command::ExpandGroup => {
            let groups = node.map(within).unwrap_or_default();
            state.expanded().update(|open| open.extend(groups));
        }
        Command::CollapseGroup => {
            let groups = node.map(within).unwrap_or_default();
            state
                .expanded()
                .update(|open| open.retain(|id| !groups.contains(id)));
        }
        Command::ExpandAll => {
            let every: HashSet<Arc<str>> = model::group_ids(state.entries()).into_iter().collect();
            state.expanded().set(every);
        }
        Command::CollapseAll => state.expanded().set(HashSet::new()),
    }
}

fn within(group: &TreeNode) -> Vec<Arc<str>> {
    TreeNode::branch_ids(std::slice::from_ref(group))
}

pub(super) fn context_layer(
    state: &WorkshopState,
    asked: RwSignal<Option<Asked>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    let layer = ReactiveList::new(
        move || vec![asked.with(Option::is_some)],
        |shown: &bool| *shown,
        move |shown| {
            let Some(asked_for) = asked.peek().filter(|_| shown) else {
                return Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?));
            };
            let props = ContextMenuProps::props()
                .on_close(Rc::new(move || asked.set(None)))
                .at(asked_for.at);
            let props = match asked_for.around {
                Some(around) => props.around(around),
                None => props,
            };
            context_menu(props.build(), rows(&state, asked_for.node))
        },
        0.0,
    )?;
    Ok(box_item(layer))
}

fn rows(state: &WorkshopState, node: Option<TreeNode>) -> Children {
    let state = state.clone();
    Children::new(move || {
        let mut slots = Slots::new();
        for command in commands(&state, node.as_ref()) {
            let row = match command {
                Some(command) => {
                    let (state, node) = (state.clone(), node.clone());
                    item(
                        ItemProps::props()
                            .label(strings::reactive(command.label()))
                            .on_press(Rc::new(move || run(&state, command, node.as_ref())))
                            .build(),
                        Children::default(),
                    )?
                }
                None => separator(SeparatorProps::props().build(), Children::default())?,
            };
            slots.push(None, row);
        }
        Ok(slots)
    })
}
