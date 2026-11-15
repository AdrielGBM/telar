//! What the UI tells the platform about itself, for whatever is listening on the other side.
//!
//! Here rather than in the UI layer for the same reason [`Event`](crate::Event) is: it is the vocabulary the two ends share. The UI declares roles and names; the platform's accessibility API — AccessKit on the desktop — is what turns them into something a screen reader can read. Neither end owns the words.

use geometry_core::Rect;
use semantics_core::LocaleTag;

/// The vocabulary itself lives a layer down, where a renderer can reach it too: the desktop announcing a checkbox and a document drawing one have to be describing the same box.
pub use semantics_core::{
    CurrentKind, HasPopup, HeadingLevel, Live, Modal, NumericValue, NumericValueError, Orientation,
    Positioning, Role, SetPosition, SortDirection, Toggle, ToggleKind, Value,
};

/// One thing a screen reader can land on.
///
/// A snapshot is a list of these that is also a tree: each node names the one it sits in by [`parent`](Self::parent), and comes after it and before that one's next sibling. Read in order, the list is the reading order a reader walks; read by its parents, it is the nesting a reader steps into — a grid's rows and their cells, a list box's options, a dialog's content.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// A stable identity within this window for as long as the widget lives. `None` for text that is not a control, which nothing needs to address.
    pub id: Option<u64>,
    /// The node this one sits in, by its place in the same snapshot, which is always before this one. `None` for a node the window holds directly.
    pub parent: Option<usize>,
    pub role: Role,
    pub name: String,
    /// Window-absolute, which is the space every platform accessibility layer works in.
    pub rect: Rect,
    pub focused: bool,
    /// `false` announces "unavailable"; a control that is genuinely not there is absent instead.
    pub enabled: bool,
    /// The on/off state a control that carries one is in — on, off or mixed — read through its role's [`Role::toggle_kind`]: checked, pressed, selected or expanded. `None` for the roles that have no such state — and never a default of [`Toggle::Off`] for the ones that do, which would announce every checkbox as unticked.
    pub toggled: Option<Toggle>,
    /// What the control shows as its value: where a slider stands and between which bounds, what a field holds, the choice a select has made.
    ///
    /// Without it a slider announces "Volume, slider" and stops — the reader can say what the control is and not what it says, which is the one thing a value control exists to report. `None` for a node that carries none.
    pub value: Option<Value>,
    /// What a reader says after the name, role and state, as help rather than identity: a field's hint, why it is invalid. `None` where nothing more is said.
    pub description: Option<String>,
    /// Whether what a field holds fails its checks.
    pub invalid: bool,
    /// Whether a field must be filled in before what it belongs to can be submitted.
    pub required: bool,
    /// Whether a field shows a value that can be read and copied but not changed.
    pub readonly: bool,
    /// The language the node is in, where the application said. `None` is the surface's own.
    pub lang: Option<LocaleTag>,
    /// Where a link goes, written as [`address_of`](crate::address_of) writes it. `None` for everything that is not a link.
    pub url: Option<String>,
    /// What a link marked current is the current one of: the page being shown, the place on it the reader is at. `None` for every box that is not a current link.
    pub current: Option<CurrentKind>,
    /// The axis a control that has one runs along: the bar of a splitter, the track of a slider. `None` for every role without an axis.
    pub orientation: Option<Orientation>,
    /// Whether a tree row with rows under it shows them. `None` for a leaf and for every role that does not expand this way.
    pub expanded: Option<bool>,
    /// The level, and the place among its siblings, of an item a reader cannot count for itself: a row of a virtualised tree.
    pub position: Option<SetPosition>,
    /// The [`id`](Self::id) of the node in this snapshot that the keyboard's cursor rests on while this one keeps focus: the row of a tree, the item of a toolbar. A reader announces it as though it held focus itself.
    pub active_descendant: Option<u64>,
    /// The [`id`](Self::id) of the node in this snapshot whose content this one decides: the list a combobox opens, the menu a button drops.
    pub controls: Option<u64>,
    /// What the node opens: the menu a menu button drops, the list a combobox opens. `None` for a node that opens nothing.
    pub has_popup: Option<HasPopup>,
    /// The chord that runs the same command as the node, in `aria-keyshortcuts` syntax (`Control+Shift+Z`). `None` for a node with no such key.
    pub keyboard_shortcut: Option<String>,
    /// Which way the rows of a grid are sorted by the column a header names. `None` on every other node.
    pub sort: Option<SortDirection>,
    /// Whether a list box, a grid or a tree lets more than one of its items be chosen at once.
    pub multiselectable: bool,
    /// Whether a dialog is inside a modal layer that is up, so a reader keeps to it as the keyboard does.
    pub modal: bool,
    /// Where the caret is, and what is selected, in the text a field holds as its [`value`](Self::value): what a reader follows as a person types and moves through it. `None` for every node but the field that holds the keyboard.
    pub selection: Option<TextSelection>,
    /// The character of the text a field holds as its [`value`](Self::value) that each of its lines after the first begins at, as the text is laid out: after a line break, and where the width wraps the line before. A reader moves through a field line by line as it is drawn. Empty for text laid out on one line, and for every node that is no field.
    pub line_starts: Vec<usize>,
    /// The actions with a value or a selection in them that the control answers, so a platform offers a reader those and no others.
    pub supports: ActionSupport,
}

/// Which [`AccessAction`]s a control answers beyond the ones every control does — [`Focus`](AccessAction::Focus), [`Activate`](AccessAction::Activate) and [`ScrollIntoView`](AccessAction::ScrollIntoView).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActionSupport {
    /// [`Increment`](AccessAction::Increment) and [`Decrement`](AccessAction::Decrement).
    pub step: bool,
    /// [`SetValue`](AccessAction::SetValue).
    pub set_value: bool,
    /// [`SetSelection`](AccessAction::SetSelection).
    pub set_selection: bool,
}

/// What a screen reader asks the control a node's [`id`](AccessNode::id) names to do. A control with no way to do it does nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessAction {
    /// Take the keyboard, as Tab arriving on it would.
    Focus,
    /// Do what a tap on it does: press a button, choose a row, follow a link.
    Activate,
    /// Move a value control one step up: a slider's step, a spin button's.
    Increment,
    /// Move a value control one step down.
    Decrement,
    /// Replace what the control holds with this: the text of a field, the reading of a slider.
    SetValue(Value),
    /// Scroll every view the control sits in until it shows.
    ScrollIntoView,
    /// Put a field's caret, or its selection, here.
    SetSelection(TextSelection),
}

/// A selection in the text a field shows, each end counted in characters — Unicode scalar values — from the start of that text: a unit every platform's text interface translates into without knowing how the field stores it. Both ends at one place is a caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    /// The end that stays put while the selection is extended.
    pub anchor: usize,
    /// The end the caret is at.
    pub focus: usize,
}

/// The nodes `parent` holds directly, by their places in `nodes`, in reading order: the window's own for `None`.
pub fn children_of(
    nodes: &[AccessNode],
    parent: Option<usize>,
) -> impl Iterator<Item = usize> + '_ {
    nodes
        .iter()
        .enumerate()
        .filter(move |(_, node)| node.parent == parent)
        .map(|(index, _)| index)
}

/// How many nodes each node of `nodes` sits inside: `0` for one the window holds directly.
pub fn depths(nodes: &[AccessNode]) -> Vec<usize> {
    let mut depths: Vec<usize> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let depth = node
            .parent
            .and_then(|parent| depths.get(parent))
            .map_or(0, |above| above + 1);
        depths.push(depth);
    }
    depths
}

/// The snapshot as a reader would speak it, one line per node in reading order, each indented two spaces for every node it sits inside: what a terminal, a log or a test can carry where there is no accessibility API to hand the nodes to.
pub fn transcript(nodes: &[AccessNode]) -> String {
    let focus = spoken_focus(nodes);
    let depths = depths(nodes);
    let mut out = String::new();
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        for _ in 0..depths[index] {
            out.push_str("  ");
        }
        out.push_str(&node.name);
        if !matches!(node.role, Role::Label | Role::KeyboardInput) {
            let role = match node.role {
                Role::Drawing => "image",
                role => role.as_str(),
            };
            if !node.name.is_empty() {
                out.push_str(", ");
            }
            out.push_str(role);
        }
        if let Role::Heading(level) = node.role {
            out.push_str(&format!(", level {}", level.get()));
        }
        if let Some(words) = node
            .value
            .as_ref()
            .map(value_words)
            .filter(|words| !words.is_empty())
        {
            out.push_str(", ");
            out.push_str(&words);
        }
        if let Some(place) = node
            .position
            .and_then(|place| Some(place_words(node.role.positioning()?, place)))
        {
            out.push_str(", ");
            out.push_str(&place);
        }
        if let Some(state) = node
            .toggled
            .and_then(|state| toggle_words(node.role, state))
        {
            out.push_str(", ");
            out.push_str(state);
        }
        if node.required {
            out.push_str(", required");
        }
        if node.invalid {
            out.push_str(", invalid");
        }
        if node.readonly {
            out.push_str(", read only");
        }
        if let Some(popup) = node.has_popup {
            out.push_str(", has ");
            out.push_str(popup.as_str());
        }
        if let Some(keys) = &node.keyboard_shortcut {
            out.push_str(", shortcut ");
            out.push_str(keys);
        }
        if let Some(open) = node.expanded {
            out.push_str(if open { ", expanded" } else { ", collapsed" });
        }
        if let Some(sort) = node.sort {
            out.push_str(", sorted ");
            out.push_str(sort.as_str());
        }
        if node.multiselectable {
            out.push_str(", multiselectable");
        }
        if node.current.is_some() {
            out.push_str(", current");
        }
        if !node.enabled {
            out.push_str(", unavailable");
        }
        if let Some(description) = &node.description {
            out.push_str(", ");
            out.push_str(description);
        }
        if focus == Some(index) {
            out.push_str(", focused");
        }
    }
    out
}

/// Where a reader says focus is: on the node holding it, or on the node its cursor rests on when it names one in the snapshot.
fn spoken_focus(nodes: &[AccessNode]) -> Option<usize> {
    let held = nodes.iter().position(|node| node.focused)?;
    let cursor = nodes[held]
        .active_descendant
        .and_then(|id| nodes.iter().position(|node| node.id == Some(id)));
    Some(cursor.unwrap_or(held))
}

/// Where an item sits, in the numbers its role takes: a reader says the place a virtualised list or grid knows and the screen does not show.
fn place_words(kind: Positioning, place: SetPosition) -> String {
    match kind {
        Positioning::Nested => format!(
            "level {}, {} of {}",
            place.level, place.position, place.size
        ),
        Positioning::InSet | Positioning::GridRow => {
            format!("{} of {}", place.position, place.size)
        }
    }
}

/// What a reader says for a value: its own words where it has them, the number otherwise.
fn value_words(value: &Value) -> String {
    match value {
        Value::Number {
            reading,
            text: None,
        } => reading.now().to_string(),
        Value::Number {
            text: Some(text), ..
        }
        | Value::Text(text) => text.to_string(),
    }
}

/// The words a reader says for a role's on/off state, or `None` where it says nothing: an unselected tab is every tab but one, and announcing each of them is noise.
fn toggle_words(role: Role, state: Toggle) -> Option<&'static str> {
    Some(match (role.toggle_kind()?, state) {
        (ToggleKind::Checked, Toggle::On) => "checked",
        (ToggleKind::Checked, Toggle::Off) => "not checked",
        (ToggleKind::Checked, Toggle::Mixed) => "partially checked",
        (ToggleKind::Pressed, Toggle::On) => "pressed",
        (ToggleKind::Pressed, Toggle::Off) => "not pressed",
        (ToggleKind::Pressed, Toggle::Mixed) => "partially pressed",
        (ToggleKind::Selected, Toggle::On) => "selected",
        (ToggleKind::Selected, Toggle::Off | Toggle::Mixed) => return None,
        (ToggleKind::Expanded, Toggle::On) => "expanded",
        (ToggleKind::Expanded, Toggle::Off | Toggle::Mixed) => "collapsed",
    })
}

#[cfg(test)]
#[path = "accessibility.test.rs"]
mod tests;
