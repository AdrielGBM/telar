//! The bridge from Telar's accessibility tree to the platform's.
//!
//! Telar describes its window as a tree of [`AccessNode`]s listed in reading order — what each control is, what it says, where it sits, and what it sits in — and this turns that into the tree AccessKit hands to a screen reader, and turns the requests coming back into ordinary input.
//!
//! Built only while something is listening. Every desktop accessibility API works this way, and it is what makes the cost honest: with no assistive technology attached, nothing here runs at all.

use accesskit::{
    Action, ActionRequest, AriaCurrent, HasPopup as AkHasPopup, Node, NodeId,
    Orientation as AkOrientation, Rect as AkRect, Role as AkRole, SortDirection as AkSortDirection,
    Toggled, TreeId, TreeInfo, TreeUpdate,
};
use platform_core::{
    AccessNode, CurrentKind, HasPopup, Orientation, Positioning, Role, SortDirection, ToggleKind,
};

/// The window itself, which every other node hangs from. A fixed id because there is exactly one and the platform needs to name it before any of its children exist.
const ROOT: NodeId = NodeId(0);

/// Turns Telar's description of the window into the tree AccessKit publishes.
///
/// The nesting is the snapshot's own — the nesting of *meaning*, a grid around its rows and a row around its cells, a list box around its options, a dialog around what it asks — not the nesting of every box a layout happened to need, and each node's children are in the reading order the snapshot lists them in. What a reader counts off a container is said on it: a grid's rows and columns, a set's size where every item in it says the same one.
pub(crate) fn tree_update(nodes: &[AccessNode], title: &str, lang: Option<&str>) -> TreeUpdate {
    let mut root = Node::new(AkRole::Window);
    root.set_label(title.to_string());
    if let Some(lang) = lang {
        root.set_language(lang);
    }

    // Positional ids for the labels, which nothing addresses; a control keeps its focus id, so the same button stays the same node across frames and a reader is not told it appeared anew.
    let ids: Vec<NodeId> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| match node.id {
            Some(focus_id) => control_id(focus_id),
            None => NodeId(index as u64 + 1),
        })
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut top = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        match node.parent.filter(|&parent| parent < index) {
            Some(parent) => children[parent].push(index),
            None => top.push(index),
        }
    }

    let mut updates: Vec<(NodeId, Node)> = Vec::with_capacity(nodes.len() + 1);
    let mut focus = ROOT;

    for (index, node) in nodes.iter().enumerate() {
        let id = ids[index];
        let mut ak = Node::new(role_of(node.role));
        ak.set_label(node.name.clone());
        ak.set_bounds(AkRect {
            x0: node.rect.x as f64,
            y0: node.rect.y as f64,
            x1: (node.rect.x + node.rect.width) as f64,
            y1: (node.rect.y + node.rect.height) as f64,
        });
        if node.id.is_some() {
            // Focus and activation are the two things a reader drives, and both come back as an `ActionRequest` this node has to have claimed.
            ak.add_action(Action::Focus);
            ak.add_action(Action::Click);
        }
        if !node.enabled {
            ak.set_disabled();
        }
        if let Some(lang) = &node.lang {
            ak.set_language(lang.as_str());
        }
        if let Some(url) = &node.url {
            ak.set_url(url.as_str());
        }
        if let Some(current) = node.current {
            ak.set_aria_current(aria_current(current));
        }
        if node.focused {
            focus = id;
        }
        // A role carrying a state has to say which, or a reader announces "checkbox" and stops. Defaulting the answer would be worse than silence, since every box would read as unticked.
        if let Some(on) = node.toggled {
            set_toggle_state(&mut ak, node.role, on);
        }
        // A slider that says only "slider" has not reported the one thing it is for.
        if let Some(v) = node.value {
            ak.set_numeric_value(v.now);
            ak.set_min_numeric_value(v.min);
            ak.set_max_numeric_value(v.max);
        }
        if let Some(orientation) = node.orientation {
            ak.set_orientation(match orientation {
                Orientation::Horizontal => AkOrientation::Horizontal,
                Orientation::Vertical => AkOrientation::Vertical,
            });
        }
        if let Some(open) = node.expanded {
            ak.set_expanded(open);
        }
        if let Some(popup) = node.has_popup {
            ak.set_has_popup(match popup {
                HasPopup::Menu => AkHasPopup::Menu,
                HasPopup::ListBox => AkHasPopup::Listbox,
            });
        }
        if let Some(keys) = &node.keyboard_shortcut {
            ak.set_keyboard_shortcut(keys.as_str());
        }
        if let Some(sort) = node.sort {
            ak.set_sort_direction(match sort {
                SortDirection::Ascending => AkSortDirection::Ascending,
                SortDirection::Descending => AkSortDirection::Descending,
            });
        }
        // AccessKit rejects a reference to a node the update does not carry.
        if let Some(item) = node
            .active_descendant
            .filter(|item| nodes.iter().any(|other| other.id == Some(*item)))
        {
            ak.set_active_descendant(control_id(item));
        }
        if let Some(target) = node
            .controls
            .filter(|target| nodes.iter().any(|other| other.id == Some(*target)))
        {
            ak.set_controls(vec![control_id(target)]);
        }
        // AccessKit counts from zero.
        if let Some((kind, place)) = node
            .position
            .and_then(|place| Some((node.role.positioning()?, place)))
        {
            match kind {
                Positioning::Nested => {
                    ak.set_level(place.level.saturating_sub(1) as usize);
                    ak.set_position_in_set(place.position.saturating_sub(1) as usize);
                }
                Positioning::InSet => {
                    ak.set_position_in_set(place.position.saturating_sub(1) as usize)
                }
                Positioning::GridRow => ak.set_row_index(place.position.saturating_sub(1) as usize),
            }
        }
        if node.multiselectable {
            ak.set_multiselectable();
        }
        if node.modal {
            ak.set_modal();
        }
        let held = &children[index];
        say_what_it_holds(&mut ak, nodes, &children, index);
        if !held.is_empty() {
            ak.set_children(held.iter().map(|&child| ids[child]).collect::<Vec<_>>());
        }
        updates.push((id, ak));
    }

    root.set_children(top.iter().map(|&index| ids[index]).collect::<Vec<_>>());
    updates.push((ROOT, root));
    TreeUpdate {
        nodes: updates,
        tree: Some(TreeInfo::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// What a reader counts off the node at `index` from the nodes the tree's `children` say each holds: a grid's rows and columns, a row's place and its cells' columns, a set's size.
///
/// The counts come from what the items say of themselves where they say it, since a virtualised grid or list builds only the items on screen; the rows and cells built are counted only where nothing says more.
fn say_what_it_holds(ak: &mut Node, nodes: &[AccessNode], children: &[Vec<usize>], index: usize) {
    let node = &nodes[index];
    let held = &children[index];
    match node.role {
        Role::Grid => {
            let rows = || {
                held.iter()
                    .copied()
                    .filter(|&child| nodes[child].role == Role::Row)
            };
            let said = rows()
                .filter_map(|row| nodes[row].position)
                .map(|place| place.size as usize)
                .max();
            ak.set_row_count(said.unwrap_or(rows().count()));
            let columns = rows()
                .map(|row| cells_of(nodes, children, row).count())
                .max()
                .unwrap_or(0);
            ak.set_column_count(columns);
        }
        Role::Row if node.position.is_none() => {
            if let Some(row) = row_index(nodes, children, index) {
                ak.set_row_index(row);
            }
        }
        Role::GridCell | Role::ColumnHeader => {
            if let Some(row) = node.parent {
                if let Some(column) = cells_of(nodes, children, row).position(|cell| cell == index)
                {
                    ak.set_column_index(column);
                }
                if let Some(row) = row_index(nodes, children, row) {
                    ak.set_row_index(row);
                }
            }
        }
        _ => {}
    }
    let mut sizes = held.iter().filter_map(|&child| {
        let item = &nodes[child];
        match item.role.positioning()? {
            Positioning::InSet | Positioning::Nested => {
                item.position.map(|place| (place.level, place.size))
            }
            Positioning::GridRow => None,
        }
    });
    if let Some(first) = sizes.next()
        && sizes.all(|other| other == first)
    {
        ak.set_size_of_set(first.1 as usize);
    }
}

/// The cells of the row at `row`, in reading order, by their places in `nodes`.
fn cells_of<'a>(
    nodes: &'a [AccessNode],
    children: &'a [Vec<usize>],
    row: usize,
) -> impl Iterator<Item = usize> + 'a {
    children[row]
        .iter()
        .copied()
        .filter(|&cell| matches!(nodes[cell].role, Role::GridCell | Role::ColumnHeader))
}

/// Where the row at `row` sits among its grid's rows, counted from zero: the place it says, or failing that its place among the rows built.
fn row_index(nodes: &[AccessNode], children: &[Vec<usize>], row: usize) -> Option<usize> {
    let node = &nodes[row];
    if node.role != Role::Row {
        return None;
    }
    if let Some(place) = node.position {
        return Some(place.position.saturating_sub(1) as usize);
    }
    let grid = node.parent.filter(|&grid| nodes[grid].role == Role::Grid)?;
    children[grid]
        .iter()
        .copied()
        .filter(|&sibling| nodes[sibling].role == Role::Row)
        .position(|sibling| sibling == row)
}
/// The node a control goes by, kept apart from the positional ids of labels.
fn control_id(focus_id: u64) -> NodeId {
    NodeId(focus_id.wrapping_add(1 << 32))
}

/// The on/off state, in the property AccessKit reads it from for this role: `toggled` is checked on a checkbox or switch and pressed on a button, while a tab is selected and a disclosure expanded.
fn set_toggle_state(ak: &mut Node, role: Role, on: bool) {
    match role.toggle_kind() {
        Some(ToggleKind::Checked | ToggleKind::Pressed) => ak.set_toggled(Toggled::from(on)),
        Some(ToggleKind::Selected) => ak.set_selected(on),
        Some(ToggleKind::Expanded) => ak.set_expanded(on),
        None => {}
    }
}

fn aria_current(current: CurrentKind) -> AriaCurrent {
    match current {
        CurrentKind::Page => AriaCurrent::Page,
        CurrentKind::Location => AriaCurrent::Location,
        CurrentKind::Item => AriaCurrent::True,
    }
}

/// Telar's roles, mapped outwards. One-way and lossy by design: Telar names what its catalogue actually has, and each platform names rather more.
///
/// The regions are here because a reader offers "jump to the navigation" and cannot until something says which box that is. They arrived with the document backend, and this is the half of that work that a screen reader on a desktop gets out of it.
fn role_of(role: Role) -> AkRole {
    match role {
        Role::Button | Role::Disclosure => AkRole::Button,
        Role::Link => AkRole::Link,
        Role::CheckBox => AkRole::CheckBox,
        Role::Radio => AkRole::RadioButton,
        Role::Switch => AkRole::Switch,
        Role::Tab => AkRole::Tab,
        Role::TabList => AkRole::TabList,
        Role::TabPanel => AkRole::TabPanel,
        Role::Menu => AkRole::Menu,
        Role::MenuItem => AkRole::MenuItem,
        Role::MenuItemCheckBox => AkRole::MenuItemCheckBox,
        Role::MenuItemRadio => AkRole::MenuItemRadio,
        Role::ListBox => AkRole::ListBox,
        Role::Option => AkRole::ListBoxOption,
        Role::Slider => AkRole::Slider,
        Role::SpinButton => AkRole::SpinButton,
        Role::TextInput => AkRole::TextInput,
        Role::MultilineTextInput => AkRole::MultilineTextInput,
        Role::ComboBox => AkRole::ComboBox,
        Role::ProgressBar => AkRole::ProgressIndicator,
        Role::Tree => AkRole::Tree,
        Role::TreeItem => AkRole::TreeItem,
        Role::Grid => AkRole::Grid,
        Role::Row => AkRole::Row,
        Role::GridCell => AkRole::GridCell,
        Role::ColumnHeader => AkRole::ColumnHeader,
        Role::Toolbar => AkRole::Toolbar,
        Role::Splitter => AkRole::Splitter,
        Role::Status => AkRole::Status,
        Role::Log => AkRole::Log,
        Role::Label => AkRole::Label,
        Role::Banner => AkRole::Banner,
        Role::Navigation => AkRole::Navigation,
        Role::Main => AkRole::Main,
        Role::Complementary => AkRole::Complementary,
        Role::ContentInfo => AkRole::ContentInfo,
        Role::Article => AkRole::Article,
        Role::Section => AkRole::Section,
        Role::Form => AkRole::Form,
        Role::Search => AkRole::SearchInput,
        Role::Heading(_) => AkRole::Heading,
        Role::List => AkRole::List,
        Role::ListItem => AkRole::ListItem,
        Role::Dialog => AkRole::Dialog,
        Role::ScrollArea => AkRole::ScrollView,
        // A picture with no name is decoration, and a reader is better off stepping over it than announcing "graphic" at every icon.
        Role::Drawing => AkRole::Image,
        Role::Group => AkRole::GenericContainer,
    }
}

/// What a reader asked for, translated back into the focus id it names. `None` for a request naming the window, a label, or a node that has since gone.
pub(crate) fn requested_focus_id(
    request: &ActionRequest,
    nodes: &[AccessNode],
) -> Option<(u64, bool)> {
    let target = request.target_node.0;
    let id = target.checked_sub(1 << 32)?;
    nodes.iter().find(|n| n.id == Some(id))?;
    let activate = matches!(request.action, Action::Click);
    Some((id, activate))
}

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod tests;
