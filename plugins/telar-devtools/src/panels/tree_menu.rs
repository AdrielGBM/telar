//! The context menu the inspection panels' trees share: opening or shutting a branch with everything under it, or the whole tree, from a secondary press or the context-menu key.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, Memo, PointerButton, Reactive,
    ReactiveList, Rect, RectStyle, RwSignal, Slots, StyledContainer, box_item, pointer_position,
    signal,
};
use telar_components::{
    ContextHandler, ContextMenuProps, ItemProps, SeparatorProps, TreeContext, TreeNode,
    context_menu, item, separator,
};

use crate::strings::{self, COLLAPSE, COLLAPSE_ALL, EXPAND, EXPAND_ALL};
use crate::workbench::workbench_fill;

/// Where the menu was asked for, and the row it is about: `None` for the space under the rows.
#[derive(Clone)]
struct Asked {
    node: Option<TreeNode>,
    at: (f32, f32),
    around: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Expand,
    Collapse,
    ExpandAll,
    CollapseAll,
}

impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::Expand => EXPAND,
            Self::Collapse => COLLAPSE,
            Self::ExpandAll => EXPAND_ALL,
            Self::CollapseAll => COLLAPSE_ALL,
        }
    }
}

/// The rows the menu shows for `node`, in order, `None` standing for a separator: a branch's own first, then the whole tree's.
pub(crate) fn commands(node: Option<&TreeNode>) -> Vec<Option<Command>> {
    let mut rows = match node {
        Some(branch) if branch.is_branch() => {
            vec![Some(Command::Expand), Some(Command::Collapse), None]
        }
        _ => Vec::new(),
    };
    rows.extend([Some(Command::ExpandAll), Some(Command::CollapseAll)]);
    rows
}

/// The menu over a tree of `items` whose open branches `expanded` holds.
#[derive(Clone, Copy)]
pub(super) struct TreeMenu {
    items: Memo<Vec<TreeNode>>,
    expanded: RwSignal<HashSet<Arc<str>>>,
    asked: RwSignal<Option<Asked>>,
}

impl TreeMenu {
    pub(super) fn new(items: Memo<Vec<TreeNode>>, expanded: RwSignal<HashSet<Arc<str>>>) -> Self {
        Self {
            items,
            expanded,
            asked: signal(None),
        }
    }

    /// What the tree calls when a row asks for its menu.
    pub(super) fn on_context(self) -> ContextHandler {
        Rc::new(move |context: TreeContext| {
            self.asked.set(Some(Asked {
                node: Some(context.node),
                at: context.at,
                around: context.around,
            }))
        })
    }

    pub(crate) fn run(self, command: Command, node: Option<&TreeNode>) {
        match command {
            Command::Expand => {
                let under = node.map(within).unwrap_or_default();
                self.expanded.update(|open| open.extend(under));
            }
            Command::Collapse => {
                let under = node.map(within).unwrap_or_default();
                self.expanded
                    .update(|open| open.retain(|id| !under.contains(id)));
            }
            Command::ExpandAll => {
                let every = self.items.with(|items| TreeNode::branch_ids(items));
                self.expanded.set(every.into_iter().collect());
            }
            Command::CollapseAll => self.expanded.set(HashSet::new()),
        }
    }

    /// `tree` with the menu over it, asked for on its rows or on the space under them.
    pub(super) fn around(
        self,
        tree: Box<dyn LayoutItem>,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let asked = self.asked;
        let body = StyledContainer::new(workbench_fill(), |_| RectStyle::default(), vec![tree])?
            .on_alt_press(move |button| {
                if button == PointerButton::Secondary {
                    asked.set(Some(Asked {
                        node: None,
                        at: pointer_position().unwrap_or_default(),
                        around: None,
                    }));
                }
            });
        Ok(box_item(Container::new(
            workbench_fill(),
            vec![box_item(body), self.layer()?],
        )?))
    }

    fn layer(self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let asked = self.asked;
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
                context_menu(props.build(), self.rows(asked_for.node))
            },
            0.0,
        )?;
        Ok(box_item(layer))
    }

    fn rows(self, node: Option<TreeNode>) -> Children {
        Children::new(move || {
            let mut slots = Slots::new();
            for command in commands(node.as_ref()) {
                let row = match command {
                    Some(command) => {
                        let node = node.clone();
                        item(
                            ItemProps::props()
                                .label(Reactive::of(move || strings::text(command.label())))
                                .on_press(Rc::new(move || self.run(command, node.as_ref())))
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
}

/// `branch` and every branch under it.
fn within(branch: &TreeNode) -> Vec<Arc<str>> {
    TreeNode::branch_ids(std::slice::from_ref(branch))
}

#[cfg(test)]
#[path = "tree_menu_test.rs"]
mod tests;
