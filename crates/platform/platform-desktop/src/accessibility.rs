//! The bridge from Telar's accessibility tree to the platform's.
//!
//! Telar describes its window as a flat list of [`AccessNode`]s — what each control is, what it says, where it sits — and this turns that into the tree AccessKit hands to a screen reader, and turns the requests coming back into ordinary input.
//!
//! Built only while something is listening. Every desktop accessibility API works this way, and it is what makes the cost honest: with no assistive technology attached, nothing here runs at all.

use accesskit::{
    Action, ActionRequest, Node, NodeId, Rect as AkRect, Role as AkRole, Toggled, TreeId, TreeInfo,
    TreeUpdate,
};
use platform_core::{AccessNode, Role};

/// The window itself, which every other node hangs from. A fixed id because there is exactly one and the platform needs to name it before any of its children exist.
const ROOT: NodeId = NodeId(0);

/// Turns Telar's flat description of the window into the tree AccessKit publishes.
///
/// Flat under one root rather than mirroring the widget hierarchy, and deliberately: the nesting a screen reader wants is the nesting of *meaning* — a control, the text explaining it — not the nesting of boxes a layout happened to need. Reading order carries that, and the nodes arrive in it.
pub(crate) fn tree_update(nodes: &[AccessNode], title: &str) -> TreeUpdate {
    let mut root = Node::new(AkRole::Window);
    root.set_label(title.to_string());

    let mut updates: Vec<(NodeId, Node)> = Vec::with_capacity(nodes.len() + 1);
    let mut children = Vec::with_capacity(nodes.len());
    let mut focus = ROOT;

    for (index, node) in nodes.iter().enumerate() {
        // Positional ids for the labels, which nothing addresses; a control keeps its focus id, so the same button stays the same node across frames and a reader is not told it appeared anew.
        let id = match node.id {
            Some(focus_id) => NodeId(focus_id.wrapping_add(1 << 32)),
            None => NodeId(index as u64 + 1),
        };
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
        if node.focused {
            focus = id;
        }
        // A role carrying a state has to say which, or a reader announces "checkbox" and stops. Defaulting the answer would be worse than silence, since every box would read as unticked.
        if let Some(on) = node.toggled {
            ak.set_toggled(if on { Toggled::True } else { Toggled::False });
        }
        // A slider that says only "slider" has not reported the one thing it is for.
        if let Some(v) = node.value {
            ak.set_numeric_value(v.now);
            ak.set_min_numeric_value(v.min);
            ak.set_max_numeric_value(v.max);
        }
        children.push(id);
        updates.push((id, ak));
    }

    root.set_children(children);
    updates.push((ROOT, root));
    TreeUpdate {
        nodes: updates,
        tree: Some(TreeInfo::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus,
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
        Role::TabPanel => AkRole::TabPanel,
        Role::MenuItem => AkRole::MenuItem,
        Role::Slider => AkRole::Slider,
        Role::SpinButton => AkRole::SpinButton,
        Role::TextInput => AkRole::TextInput,
        Role::MultilineTextInput => AkRole::MultilineTextInput,
        Role::ComboBox => AkRole::ComboBox,
        Role::ProgressBar => AkRole::ProgressIndicator,
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
