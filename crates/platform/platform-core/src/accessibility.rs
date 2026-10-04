//! What the UI tells the platform about itself, for whatever is listening on the other side.
//!
//! Here rather than in the UI layer for the same reason [`Event`](crate::Event) is: it is the vocabulary the two ends share. The UI declares roles and names; the platform's accessibility API — AccessKit on the desktop — is what turns them into something a screen reader can read. Neither end owns the words.

use geometry_core::Rect;

/// The vocabulary itself lives a layer down, where a renderer can reach it too: the desktop announcing a checkbox and a document drawing one have to be describing the same box.
pub use semantics_core::{Role, ToggleKind};

/// One thing a screen reader can land on.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// A stable identity within this window for as long as the widget lives. `None` for text that is not a control, which nothing needs to address.
    pub id: Option<u64>,
    pub role: Role,
    pub name: String,
    /// Window-absolute, which is the space every platform accessibility layer works in.
    pub rect: Rect,
    pub focused: bool,
    /// `false` announces "unavailable"; a control that is genuinely not there is absent instead.
    pub enabled: bool,
    /// Whether a control that carries an on/off state is in it, read through its role's [`Role::toggle_kind`]: checked, pressed, selected or expanded. `None` for the roles that have no such state — and never a default of `false` for the ones that do, which would announce every checkbox as unticked.
    pub toggled: Option<bool>,
    /// Where a control that carries a number stands, and between which bounds.
    ///
    /// Without it a slider announces "Volume, slider" and stops — the reader can say what the control is and not what it says, which is the one thing a value control exists to report. `None` for the roles that carry no number.
    pub value: Option<NumericValue>,
    /// The language the node is in, as a BCP 47 tag, where the application said. `None` is the surface's own.
    pub lang: Option<String>,
    /// Where a link goes, written as [`address_of`](crate::address_of) writes it. `None` for everything that is not a link.
    pub url: Option<String>,
}

/// A numeric control's reading: where it is now, and the range that makes that number mean something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericValue {
    pub now: f64,
    pub min: f64,
    pub max: f64,
}

/// The snapshot as a reader would speak it, one line per node in reading order: what a terminal, a log or a test can carry where there is no accessibility API to hand the nodes to.
pub fn transcript(nodes: &[AccessNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&node.name);
        if node.role != Role::Label {
            let role = match node.role {
                Role::Drawing => "image",
                role => role.as_str(),
            };
            out.push_str(", ");
            out.push_str(role);
        }
        if let Some(state) = node.toggled.and_then(|on| toggle_words(node.role, on)) {
            out.push_str(", ");
            out.push_str(state);
        }
        if !node.enabled {
            out.push_str(", unavailable");
        }
        if node.focused {
            out.push_str(", focused");
        }
    }
    out
}

/// The words a reader says for a role's on/off state, or `None` where it says nothing: an unselected tab is every tab but one, and announcing each of them is noise.
fn toggle_words(role: Role, on: bool) -> Option<&'static str> {
    Some(match (role.toggle_kind()?, on) {
        (ToggleKind::Checked, true) => "checked",
        (ToggleKind::Checked, false) => "not checked",
        (ToggleKind::Pressed, true) => "pressed",
        (ToggleKind::Pressed, false) => "not pressed",
        (ToggleKind::Selected, true) => "selected",
        (ToggleKind::Selected, false) => return None,
        (ToggleKind::Expanded, true) => "expanded",
        (ToggleKind::Expanded, false) => "collapsed",
    })
}

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod tests;
