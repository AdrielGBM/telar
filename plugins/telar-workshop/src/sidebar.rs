//! The sidebar: a search field over the previews, grouped by their title path in a tree.

mod model;

use std::cell::Cell;
use std::rc::Rc;

use telar::focus::{self, Role};
use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Container, Key, LayoutError, LayoutItem,
    LayoutStyle, Memo, NamedKey, NodeId, Reactive, ReactiveList, RectStyle, StyledContainer, Text,
    box_item, children, effect, memo, signal, use_theme_tokens,
};
use telar_components::{
    SearchFieldProps, TrailingBuilder, TreeNode, TreeViewProps, search_field, tree_view,
};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, workbench_muted};

use crate::snapshot::drift_badge;
use crate::state::{SIDEBAR_MAX_WIDTH, WorkshopState};
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
        LayoutStyle::column().flex_grow(1.0).min_height(0.0),
        move || vec![items.with(Vec::is_empty)],
        |is_empty: &bool| *is_empty,
        move |is_empty| match is_empty {
            true => empty(&body_panel),
            false => tree(&body_panel),
        },
    )?;
    let sidebar = Container::new(
        LayoutStyle::column().flex_grow(1.0).min_height(0.0),
        vec![
            box_item(heading),
            search_field_item(&panel)?,
            box_item(body),
        ],
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

    /// Selects the first preview the search finds and takes the keyboard to it in the tree; from the drawer, it shuts the drawer instead, so the canvas showing it is in view.
    fn select_first_match(&self) {
        let first = self
            .items
            .with(|items| first_leaf(items).map(|leaf| leaf.id.clone()));
        if let Some(id) = first {
            self.state.select(&id);
            if !self.close_drawer() {
                self.focus_tree();
            }
        }
    }

    /// Shuts the sidebar's drawer, and answers whether it was open.
    fn close_drawer(&self) -> bool {
        let drawer = self.state.sidebar_drawer();
        let open = drawer.peek();
        drawer.set_if_changed(false);
        open
    }
}

fn first_leaf(nodes: &[TreeNode]) -> Option<&TreeNode> {
    nodes.iter().find_map(|node| match node.is_branch() {
        true => first_leaf(&node.children),
        false => Some(node),
    })
}

fn search_field_item(panel: &Rc<Panel>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keys = panel.clone();
    let submit = panel.clone();
    let search = panel.state.search();
    let focus = signal(None);
    let field = search_field(
        SearchFieldProps::props()
            .value(search)
            .width(SIDEBAR_MAX_WIDTH)
            .placeholder(Reactive::of(|| strings::text(SEARCH)))
            .focus_id(focus)
            .on_submit(Rc::new(move |_: &str| submit.select_first_match()))
            .on_key(Rc::new(move |key: &Key, _modifiers| match key {
                Key::Named(NamedKey::ArrowDown) => {
                    keys.focus_tree();
                    true
                }
                Key::Named(NamedKey::Escape) if search.with(String::is_empty) => {
                    keys.focus_tree();
                    true
                }
                _ => false,
            }))
            .build(),
        Children::default(),
    )?;
    if let Some(id) = focus.peek() {
        panel.state.set_search_field(id);
    }
    let margin = Container::new(
        LayoutStyle::column()
            .flex_shrink(0.0)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_bottom(WORKBENCH_GRID),
        children![field],
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
    let mut children = children![message];
    if !panel.state.entries().is_empty() {
        let search = panel.state.search();
        let label = Text::declaring(
            || strings::text(CLEAR_SEARCH),
            LayoutStyle::new(),
            |text| text.with_color(use_theme_tokens().primary()),
        )?;
        let clear = StyledContainer::new(
            LayoutStyle::row()
                .padding_horizontal(WORKBENCH_GRID)
                .padding_vertical(WORKBENCH_GRID / 2.0),
            |_| RectStyle::default().with_radius(BorderRadius::all(WORKBENCH_RADIUS)),
            children![label],
        )?
        .control(Role::Button)
        .a11y_label(|| strings::text(CLEAR_SEARCH))
        .on_press(move || search.set(String::new()));
        children.push(box_item(clear));
    }
    let column = Container::new(
        LayoutStyle::column()
            .align_items(AlignItems::START)
            .padding_horizontal(WORKBENCH_GRID * 2.0)
            .gap(WORKBENCH_GRID),
        children,
    )?;
    Ok(box_item(column))
}

/// Marks each preview a snapshot of which the last test run failed.
fn drift_badges(state: &WorkshopState) -> TrailingBuilder {
    let review = state.snapshots().clone();
    Rc::new(move |node: &TreeNode| drift_badge(&review, &node.id))
}

fn tree(panel: &Rc<Panel>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = &panel.state;
    let picked = panel.clone();
    let tree = tree_view(
        TreeViewProps::props()
            .items(panel.items)
            .selected(state.selection())
            .expanded(state.expanded())
            .on_select(Rc::new(move |_: &TreeNode| {
                picked.close_drawer();
            }))
            .select_branches(false)
            .trailing(drift_badges(state))
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
