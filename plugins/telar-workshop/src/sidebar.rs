//! The sidebar: a search field over the previews, grouped by their title path in a tree.

mod model;

use std::cell::Cell;
use std::rc::Rc;

use telar::focus::{self, Role};
use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Input, Key, LayoutError, LayoutItem,
    LayoutStyle, Memo, NamedKey, NodeId, Reactive, ReactiveList, RectStyle, StyledContainer, Text,
    box_item, effect, memo, single_line_box, use_theme_tokens,
};
use telar_components::{TreeNode, TreeViewProps, tree_view};
use telar_devtools::{
    WORKBENCH_GRID, WORKBENCH_RADIUS, WORKBENCH_TEXT_SIZE, workbench_card, workbench_muted,
};

use crate::state::WorkshopState;
use crate::strings::{self, CLEAR_SEARCH, NO_MATCHES, NO_PREVIEWS, PREVIEWS, SEARCH};

pub(crate) use model::{group_ids, listed};

/// What fills the sidebar pane the shell sizes.
pub(crate) fn sidebar(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = Text::declaring(
        || strings::text(PREVIEWS),
        LayoutStyle::new().padding_all(WORKBENCH_GRID),
        workbench_muted,
    )?;
    let panel = Rc::new(Panel::new(state));
    panel.reveal_matches();
    let items = panel.items;
    let body_panel = panel.clone();
    let body = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0),
        move || vec![items.with(Vec::is_empty)],
        |is_empty: &bool| *is_empty,
        move |is_empty| match is_empty {
            true => empty(&body_panel),
            false => tree(&body_panel),
        },
    )?;
    let sidebar = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0),
        |_| RectStyle::default(),
        vec![box_item(heading), search_field(&panel)?, box_item(body)],
    )?;
    Ok(box_item(sidebar))
}

/// The hook for a status badge on a preview row: the text it carries, or `None` for no badge.
fn badge(_entry: &PreviewEntry) -> Option<String> {
    None
}

struct Panel {
    state: WorkshopState,
    items: Memo<Vec<TreeNode>>,
    tree: Cell<Option<NodeId>>,
}

impl Panel {
    fn new(state: &WorkshopState) -> Self {
        let search = state.search();
        let source = state.clone();
        let items = memo(move || search.with(|query| model::tree(source.entries(), query, badge)));
        Self {
            state: state.clone(),
            items,
            tree: Cell::new(None),
        }
    }

    /// Opens every group holding a match of the search, so what the search found is on screen.
    fn reveal_matches(&self) {
        let (search, expanded, items) = (self.state.search(), self.state.expanded(), self.items);
        effect(move || {
            if search.with(|query| query.trim().is_empty()) {
                return;
            }
            let mut groups = Vec::new();
            items.with(|items| model::collect_branches(items, &mut groups));
            if !expanded.peek_with(|open| groups.iter().all(|id| open.contains(id))) {
                expanded.update(|open| open.extend(groups));
            }
        });
    }

    fn focus_tree(&self) {
        if let Some(tree) = self.tree.get() {
            focus::focus_first_in(tree);
        }
    }

    fn select_first_match(&self) {
        let first = self
            .items
            .with(|items| first_leaf(items).map(|leaf| leaf.id.clone()));
        if let Some(id) = first {
            self.state.select(&id);
            self.focus_tree();
        }
    }
}

fn first_leaf(nodes: &[TreeNode]) -> Option<&TreeNode> {
    nodes.iter().find_map(|node| match node.is_branch() {
        true => first_leaf(&node.children),
        false => Some(node),
    })
}

fn search_field(panel: &Rc<Panel>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keys = panel.clone();
    let submit = panel.clone();
    let search = panel.state.search();
    let input = Input::declaring(
        search,
        LayoutStyle::new().height(single_line_box(WORKBENCH_TEXT_SIZE)),
        |text| text,
    )?
    .placeholder(strings::text(SEARCH))
    .a11y_label(|| strings::text(SEARCH))
    .on_submit(move || submit.select_first_match())
    .on_key(move |key, _modifiers| match key {
        Key::Named(NamedKey::ArrowDown) => {
            keys.focus_tree();
            true
        }
        Key::Named(NamedKey::Escape) => {
            if search.with(String::is_empty) {
                keys.focus_tree();
            } else {
                search.set(String::new());
            }
            true
        }
        _ => false,
    });
    let focus = input.focus_id();
    panel.state.set_search_field(focus);
    let field = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0)
            .bordered(),
        |_| workbench_card(),
        vec![box_item(input)],
    )?
    .frames_focus_of(focus);
    let margin = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_shrink(0.0)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_bottom(WORKBENCH_GRID),
        |_| RectStyle::default(),
        vec![box_item(field)],
    )?;
    Ok(box_item(margin))
}

fn empty(panel: &Rc<Panel>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    panel.tree.set(None);
    let message_key = if panel.state.entries().is_empty() {
        NO_PREVIEWS
    } else {
        NO_MATCHES
    };
    let message = Text::declaring(
        move || strings::text(message_key),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let mut children = vec![box_item(message)];
    if !panel.state.entries().is_empty() {
        let search = panel.state.search();
        let label = Text::declaring(
            || strings::text(CLEAR_SEARCH),
            LayoutStyle::new(),
            |text| text.with_color(use_theme_tokens().primary()),
        )?;
        let clear = StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .padding_horizontal(WORKBENCH_GRID)
                .padding_vertical(WORKBENCH_GRID / 2.0),
            |_| RectStyle::default().with_radius(BorderRadius::all(WORKBENCH_RADIUS)),
            vec![box_item(label)],
        )?
        .control(Role::Button)
        .a11y_label(|| strings::text(CLEAR_SEARCH))
        .on_press(move || search.set(String::new()));
        children.push(box_item(clear));
    }
    let column = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::START)
            .padding_horizontal(WORKBENCH_GRID * 2.0)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        children,
    )?;
    Ok(box_item(column))
}

fn tree(panel: &Rc<Panel>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = &panel.state;
    let tree = tree_view(
        TreeViewProps::props()
            .items(panel.items)
            .selected(state.selection())
            .expanded(state.expanded())
            .select_branches(false)
            .label(Reactive::of(|| strings::text(PREVIEWS)))
            .build(),
        Children::default(),
    )?;
    panel.tree.set(Some(tree.layout_node()));
    Ok(tree)
}

#[cfg(test)]
#[path = "sidebar_test.rs"]
mod tests;
