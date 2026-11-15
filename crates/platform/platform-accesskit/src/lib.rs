//! The bridge from Telar's accessibility tree to AccessKit's, which the desktop and Android backends both publish through their platform's adapter.
//!
//! Telar describes its window as a tree of [`AccessNode`]s listed in reading order — what each control is, what it says, where it sits, and what it sits in — and this turns that into the tree AccessKit hands to a screen reader, and turns the requests coming back into ordinary input. One mapping for every platform, so a control reads and answers the same whichever one it runs on.
//!
//! Built only once something has asked for the tree: with no assistive technology attached, nothing here runs at all.

#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::num::NonZeroU32;

/// The AccessKit the tree is built from, so a platform adapter names the very version this crate publishes to.
pub use accesskit;

use accesskit::{
    Action, ActionData, ActionRequest, Affine, AriaCurrent, HasPopup as AkHasPopup, Invalid,
    Live as AkLive, Node, NodeId, Orientation as AkOrientation, Rect as AkRect, Role as AkRole,
    SortDirection as AkSortDirection, TextPosition, TextSelection as AkTextSelection, Toggled,
    TreeId, TreeInfo, TreeUpdate,
};
use platform_core::{
    AccessAction, AccessId, AccessNode, ActionSupport, CurrentKind, HasPopup, Live, LocaleTag,
    NumericValue, Orientation, Positioning, Role, SortDirection, TextSelection, Toggle, ToggleKind,
    Value,
};

/// The window itself, which every other node hangs from. A fixed id because there is exactly one and the platform needs to name it before any of its children exist.
const ROOT: NodeId = NodeId(OWN);

/// The bit set on every id the platform gives a node of its own, the window and the labels, and clear on every id a control goes by.
const OWN: u64 = 1 << 63;

/// Turns Telar's description of the window into the tree AccessKit publishes.
///
/// The nesting is the snapshot's own — the nesting of *meaning*, a grid around its rows and a row around its cells, a list box around its options, a dialog around what it asks — not the nesting of every box a layout happened to need, and each node's children are in the reading order the snapshot lists them in. What a reader counts off a container is said on it: a grid's rows and columns, a set's size where every item in it says the same one.
///
/// `scale_factor` is the window's physical pixels per logical one: the snapshot is laid out in logical pixels and every adapter reads bounds in physical ones, so the window scales its whole tree by it and a reader's highlight and a touch to explore land on the control drawn there.
pub fn tree_update(
    nodes: &[AccessNode],
    title: &str,
    lang: Option<&LocaleTag>,
    scale_factor: f64,
) -> TreeUpdate {
    let mut root = Node::new(AkRole::Window);
    root.set_transform(Affine::scale(scale_factor));
    root.set_label(title.to_string());
    if let Some(lang) = lang {
        root.set_language(lang.to_string());
    }

    // Positional ids for the labels, which nothing addresses; a control keeps its focus id, so the same button stays the same node across frames and a reader is not told it appeared anew.
    let controls: Vec<Option<NodeId>> = nodes
        .iter()
        .map(|node| node.id.filter(|&id| is_control_id(id)).map(control_id))
        .collect();
    let ids: Vec<NodeId> = controls
        .iter()
        .enumerate()
        .map(|(index, control)| control.unwrap_or_else(|| label_id(index)))
        .collect();
    let control_named = |id: AccessId| {
        nodes
            .iter()
            .position(|other| other.id == Some(id))
            .and_then(|at| controls[at])
    };
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut top = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        match node.parent.filter(|&parent| parent < index) {
            Some(parent) => children[parent].push(index),
            None => top.push(index),
        }
    }

    let runs = text_runs(nodes);
    let mut updates: Vec<(NodeId, Node)> = Vec::with_capacity(nodes.len() + runs.len() + 1);
    let mut focus = ROOT;

    for (index, node) in nodes.iter().enumerate() {
        let id = ids[index];
        let mut ak = Node::new(role_of(&node.role));
        ak.set_label(node.name.clone());
        ak.set_bounds(AkRect {
            x0: node.rect.x as f64,
            y0: node.rect.y as f64,
            x1: (node.rect.x + node.rect.width) as f64,
            y1: (node.rect.y + node.rect.height) as f64,
        });
        if controls[index].is_some() {
            claim_actions(&mut ak, node.supports);
        }
        if !node.enabled {
            ak.set_disabled();
        }
        if let Some(lang) = &node.lang {
            ak.set_language(lang.to_string());
        }
        if let Some(link) = node.role.link() {
            if let Some(destination) = &link.destination {
                ak.set_url(platform_core::address_of(destination));
            }
            if let Some(current) = link.current_kind() {
                ak.set_aria_current(aria_current(current));
            }
        }
        if node.focused {
            focus = id;
        }
        // A role carrying a state has to say which, or a reader announces "checkbox" and stops. Defaulting the answer would be worse than silence, since every box would read as unticked.
        if let Some(state) = node.toggled {
            set_toggle_state(&mut ak, &node.role, state);
        }
        // A slider that says only "slider" has not reported the one thing it is for.
        if let Some(value) = &node.value {
            set_value(&mut ak, value);
        }
        if let Some(description) = &node.description {
            ak.set_description(description.as_str());
        }
        if node.invalid {
            ak.set_invalid(Invalid::True);
        }
        if node.required {
            ak.set_required();
        }
        if node.readonly {
            ak.set_read_only();
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
        if let Some(item) = node.active_descendant.and_then(control_named) {
            ak.set_active_descendant(item);
        }
        if let Some(target) = node.controls.and_then(control_named) {
            ak.set_controls(vec![target]);
        }
        if let Some(live) = node.role.live() {
            ak.set_live(match live {
                Live::Polite => AkLive::Polite,
                Live::Assertive => AkLive::Assertive,
            });
        }
        if let Role::Heading(level) = &node.role {
            ak.set_level(usize::from(level.get() - 1));
        }
        if let Some((kind, place)) = node
            .position
            .and_then(|place| Some((node.role.positioning()?, place)))
        {
            match kind {
                Positioning::Nested => {
                    ak.set_level(from_zero(place.level));
                    ak.set_position_in_set(from_zero(place.position));
                }
                Positioning::InSet => ak.set_position_in_set(from_zero(place.position)),
                Positioning::GridRow => ak.set_row_index(from_zero(place.position)),
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
        let lines: Vec<NodeId> = runs
            .iter()
            .filter(|run| run.field == index)
            .map(|run| run.id)
            .collect();
        if let Some((anchor, focus)) = node.selection.and_then(|selection| {
            Some((
                position_in(&runs, index, selection.anchor)?,
                position_in(&runs, index, selection.focus)?,
            ))
        }) {
            ak.set_text_selection(AkTextSelection { anchor, focus });
        }
        if !held.is_empty() || !lines.is_empty() {
            let mut kids: Vec<NodeId> = held.iter().map(|&child| ids[child]).collect();
            kids.extend(lines);
            ak.set_children(kids);
        }
        updates.push((id, ak));
    }
    for run in &runs {
        updates.push((run.id, run_node(run, nodes[run.field].rect)));
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
    match &node.role {
        Role::Grid => {
            let rows = || {
                held.iter()
                    .copied()
                    .filter(|&child| nodes[child].role == Role::Row)
            };
            let said = rows()
                .filter_map(|row| nodes[row].position)
                .map(|place| place.size.get() as usize)
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
        ak.set_size_of_set(first.1.get() as usize);
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
        return Some(from_zero(place.position));
    }
    let grid = node.parent.filter(|&grid| nodes[grid].role == Role::Grid)?;
    children[grid]
        .iter()
        .copied()
        .filter(|&sibling| nodes[sibling].role == Role::Row)
        .position(|sibling| sibling == row)
}
/// A count from one as AccessKit counts it, from zero.
fn from_zero(counted: NonZeroU32) -> usize {
    (counted.get() - 1) as usize
}

/// The node a control goes by: its id in the snapshot, as the one integer [`AccessId::to_bits`] makes of it.
fn control_id(id: AccessId) -> NodeId {
    NodeId(id.to_bits())
}

/// Whether a control's id is clear of the ids the platform gives its own nodes; one that is not, from a surface key past `1 << 31`, is read as a label, which nothing addresses.
fn is_control_id(id: AccessId) -> bool {
    id.to_bits() & OWN == 0
}

/// The node of the label read at `index` in the snapshot.
fn label_id(index: usize) -> NodeId {
    NodeId(OWN | (index as u64 + 1))
}

/// The value as numbers where it is one, and as the words a reader says for it where it has any: a field's text, a select's choice, a slider's "Medium".
fn set_value(ak: &mut Node, value: &Value) {
    if let Some(reading) = value.reading() {
        ak.set_numeric_value(reading.now());
        ak.set_min_numeric_value(reading.min());
        ak.set_max_numeric_value(reading.max());
    }
    if let Some(text) = value.text() {
        ak.set_value(text);
    }
}

/// The on/off state, in the property AccessKit reads it from for this role: `toggled` is checked on a checkbox or switch and pressed on a button, while a tab is selected and a disclosure expanded.
fn set_toggle_state(ak: &mut Node, role: &Role, state: Toggle) {
    let on = state == Toggle::On;
    match role.toggle_kind() {
        Some(ToggleKind::Checked | ToggleKind::Pressed) => ak.set_toggled(match state {
            Toggle::Off => Toggled::False,
            Toggle::On => Toggled::True,
            Toggle::Mixed => Toggled::Mixed,
        }),
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
/// The regions are here because a reader offers "jump to the navigation" and cannot until something says which box that is. They arrived with the document backend, and this is the half of that work that a screen reader on the desktop and on Android gets out of it.
fn role_of(role: &Role) -> AkRole {
    match role {
        Role::Button | Role::Disclosure => AkRole::Button,
        Role::Link(_) => AkRole::Link,
        Role::CheckBox => AkRole::CheckBox,
        Role::Radio => AkRole::RadioButton,
        Role::RadioGroup => AkRole::RadioGroup,
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
        Role::Alert => AkRole::Alert,
        Role::Label => AkRole::Label,
        // Static text, so a reader reads its name where a generic container's may be passed over.
        Role::KeyboardInput => AkRole::Label,
        Role::Banner => AkRole::Banner,
        Role::Navigation => AkRole::Navigation,
        Role::Main => AkRole::Main,
        Role::Complementary => AkRole::Complementary,
        Role::ContentInfo => AkRole::ContentInfo,
        Role::Article => AkRole::Article,
        Role::Section => AkRole::Section,
        Role::Form => AkRole::Form,
        Role::Search => AkRole::Search,
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

/// Claims on a control's node every action a reader can ask of it and have answered: the ones every control answers, and those of `supports`.
fn claim_actions(ak: &mut Node, supports: ActionSupport) {
    ak.add_action(Action::Focus);
    ak.add_action(Action::Click);
    ak.add_action(Action::ScrollIntoView);
    if supports.step {
        ak.add_action(Action::Increment);
        ak.add_action(Action::Decrement);
    }
    if supports.set_value {
        ak.add_action(Action::SetValue);
    }
    if supports.set_selection {
        ak.add_action(Action::SetTextSelection);
    }
}

/// One line of the text a field holds as it is laid out, as the run a reader moves its caret through.
struct TextRun {
    /// The field, by its place in the snapshot.
    field: usize,
    id: NodeId,
    /// Where the line begins in the field's text, in characters.
    start: usize,
    /// The line, with the line break that ends it where one does.
    text: String,
}

/// The runs of every field holding text, one per line it is laid out in, numbered after the labels: a reader reads a field's text, and places its caret in it, only through runs.
fn text_runs(nodes: &[AccessNode]) -> Vec<TextRun> {
    let mut runs = Vec::new();
    for (field, node) in nodes.iter().enumerate() {
        if !matches!(node.role, Role::TextInput | Role::MultilineTextInput) {
            continue;
        }
        let Some(Value::Text(text)) = &node.value else {
            continue;
        };
        let characters: Vec<char> = text.chars().collect();
        let ends = node.line_starts.iter().copied().chain([characters.len()]);
        let mut start = 0;
        for end in ends {
            let end = end.clamp(start, characters.len());
            runs.push(TextRun {
                field,
                id: label_id(nodes.len() + runs.len()),
                start,
                text: characters[start..end].iter().collect(),
            });
            start = end;
        }
    }
    runs
}

fn run_node(run: &TextRun, rect: geometry_core::Rect) -> Node {
    let mut ak = Node::new(AkRole::TextRun);
    ak.set_value(run.text.as_str());
    ak.set_character_lengths(
        run.text
            .chars()
            .map(|character| character.len_utf8() as u8)
            .collect::<Vec<u8>>(),
    );
    ak.set_bounds(AkRect {
        x0: rect.x as f64,
        y0: rect.y as f64,
        x1: (rect.x + rect.width) as f64,
        y1: (rect.y + rect.height) as f64,
    });
    ak
}

/// Where the character `at` of the text of the field at `field` falls among its runs: on the line break at the end of a line rather than after it, as a caret at the end of that line stands.
fn position_in(runs: &[TextRun], field: usize, at: usize) -> Option<TextPosition> {
    let run = runs
        .iter()
        .rfind(|run| run.field == field && run.start <= at)?;
    Some(TextPosition {
        node: run.id,
        character_index: (at - run.start).min(run.text.chars().count()),
    })
}

/// The field, by its place in the snapshot, and the character of its text a position in one of its runs names.
fn character_at(runs: &[TextRun], position: TextPosition) -> Option<(usize, usize)> {
    let run = runs.iter().find(|run| run.id == position.node)?;
    Some((run.field, run.start + position.character_index))
}

/// What a reader asked for, as the focus id of the control it names and the action it asked of it. `None` for a request naming the window or a label, one for a node that has since gone, and one for an action no control answers.
pub fn requested_action(
    request: &ActionRequest,
    nodes: &[AccessNode],
) -> Option<(AccessId, AccessAction)> {
    if request.target_node.0 & OWN != 0 {
        return None;
    }
    let id = AccessId::from_bits(request.target_node.0);
    let target = nodes.iter().position(|n| n.id == Some(id))?;
    let action = match (request.action, &request.data) {
        (Action::Focus, _) => AccessAction::Focus,
        (Action::Click, _) => AccessAction::Activate,
        (Action::Increment, _) => AccessAction::Increment,
        (Action::Decrement, _) => AccessAction::Decrement,
        (Action::ScrollIntoView, _) => AccessAction::ScrollIntoView,
        (Action::SetValue, Some(ActionData::Value(text))) => {
            AccessAction::SetValue(Value::Text(text.as_ref().into()))
        }
        (Action::SetValue, Some(ActionData::NumericValue(now))) => {
            let reading = match nodes[target].value.as_ref().and_then(Value::reading) {
                Some(bounds) => NumericValue::new(
                    bounds.min()..=bounds.max(),
                    now.clamp(bounds.min(), bounds.max()),
                ),
                None => NumericValue::new(*now..=*now, *now),
            };
            AccessAction::SetValue(Value::from(reading.ok()?))
        }
        (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
            let runs = text_runs(nodes);
            let (field, anchor) = character_at(&runs, selection.anchor)?;
            let (other, focus) = character_at(&runs, selection.focus)?;
            if field != target || other != target {
                return None;
            }
            AccessAction::SetSelection(TextSelection { anchor, focus })
        }
        _ => return None,
    };
    Some((id, action))
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
