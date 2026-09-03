//! The bridge from Telar's accessibility tree to the platform's.
//!
//! Telar describes its window as a flat list of [`AccessNode`]s — what each control is, what it says, where it
//! sits — and this turns that into the tree AccessKit hands to a screen reader, and turns the requests coming
//! back into ordinary input.
//!
//! Built only while something is listening. Every desktop accessibility API works this way, and it is what
//! makes the cost honest: with no assistive technology attached, nothing here runs at all.

use accesskit::{
    Action, ActionRequest, Node, NodeId, Rect as AkRect, Role as AkRole, Toggled, Tree, TreeId,
    TreeUpdate,
};
use platform_core::{AccessNode, Role};

/// The window itself, which every other node hangs from. A fixed id because there is exactly one and the
/// platform needs to name it before any of its children exist.
const ROOT: NodeId = NodeId(0);

/// Turns Telar's flat description of the window into the tree AccessKit publishes.
///
/// Flat under one root rather than mirroring the widget hierarchy, and deliberately: the nesting a screen
/// reader wants is the nesting of *meaning* — a control, the text explaining it — not the nesting of boxes a
/// layout happened to need. Reading order carries that, and the nodes arrive in it.
pub(crate) fn tree_update(nodes: &[AccessNode], title: &str) -> TreeUpdate {
    let mut root = Node::new(AkRole::Window);
    root.set_label(title.to_string());

    let mut updates: Vec<(NodeId, Node)> = Vec::with_capacity(nodes.len() + 1);
    let mut children = Vec::with_capacity(nodes.len());
    let mut focus = ROOT;

    for (index, node) in nodes.iter().enumerate() {
        // Positional ids for the labels, which nothing addresses; a control keeps its focus id, so the same
        // button stays the same node across frames and a reader is not told it appeared anew.
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
            // Focus and activation are the two things a reader drives, and both come back as an
            // `ActionRequest` this node has to have claimed.
            ak.add_action(Action::Focus);
            ak.add_action(Action::Click);
        }
        if !node.enabled {
            ak.set_disabled();
        }
        if node.focused {
            focus = id;
        }
        // A role that carries a state has to say which, or a reader announces "checkbox" and stops there —
        // and defaulting the answer would be worse than silence, since every box would read as unticked.
        if let Some(on) = node.toggled {
            ak.set_toggled(if on { Toggled::True } else { Toggled::False });
        }
        // Same reason as the state above: a slider that says only "slider" has not reported the one thing it is for.
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
        tree: Some(Tree::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// Telar's roles, mapped outwards. One-way and lossy by design: Telar names what its catalogue actually has,
/// and each platform names rather more.
///
/// The regions are here because a reader offers "jump to the navigation" and cannot until something says
/// which box that is. They arrived with the document backend, and this is the half of that work that a
/// screen reader on a desktop gets out of it.
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
        // A picture with no name is decoration, and a reader is better off stepping over it than announcing
        // "graphic" at every icon. One that has a name arrives as a labelled image.
        Role::Drawing => AkRole::Image,
        Role::Group => AkRole::GenericContainer,
    }
}

/// What a reader asked for, translated back into the focus id it names. `None` for a request naming the
/// window, a label, or a node that has since gone.
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
mod tests {
    use geometry_core::Rect;

    use super::*;

    fn node(id: Option<u64>, role: Role, name: &str) -> AccessNode {
        AccessNode {
            id,
            role,
            name: name.to_string(),
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            focused: false,
            enabled: true,
            toggled: None,
            value: None,
        }
    }

    /// The shape a screen reader is handed: one window, every control and every piece of text under it, and
    /// each control addressable so focus and activation can come back.
    #[test]
    fn the_window_is_the_root_and_everything_hangs_from_it() {
        let nodes = vec![
            node(Some(7), Role::Button, "Save"),
            node(None, Role::Label, "Unsaved changes"),
        ];
        let update = tree_update(&nodes, "Editor");

        assert_eq!(update.nodes.len(), 3, "two nodes plus the window");
        let (root_id, root) = update.nodes.last().unwrap();
        assert_eq!(*root_id, ROOT);
        assert_eq!(root.role(), AkRole::Window);
        assert_eq!(root.children().len(), 2);
        assert_eq!(update.nodes[0].1.role(), AkRole::Button);
        assert_eq!(update.nodes[1].1.role(), AkRole::Label);
    }

    /// A control keeps its identity across frames, so a reader is not told the button appeared anew every time
    /// anything else on screen moved. The label beside it has no identity to keep and does not need one.
    #[test]
    fn a_control_keeps_the_same_node_id_between_updates() {
        let first = tree_update(&[node(Some(7), Role::Button, "Save")], "Editor");
        let with_extra = tree_update(
            &[
                node(None, Role::Label, "Heading"),
                node(Some(7), Role::Button, "Save"),
            ],
            "Editor",
        );
        let button = |u: &TreeUpdate| {
            u.nodes
                .iter()
                .find(|(_, n)| n.role() == AkRole::Button)
                .unwrap()
                .0
        };
        assert_eq!(button(&first), button(&with_extra));
    }

    /// The focused control is what the reader is told about first, and the window stands in when nothing is.
    #[test]
    fn focus_points_at_the_focused_control_or_at_the_window() {
        assert_eq!(tree_update(&[], "Editor").focus, ROOT);

        let mut focused = node(Some(7), Role::Button, "Save");
        focused.focused = true;
        let update = tree_update(&[focused], "Editor");
        assert_ne!(update.focus, ROOT);
        assert_eq!(update.focus, update.nodes[0].0);
    }

    /// A request coming back names a node; only a control has a focus id behind it, and one that has gone from
    /// the tree since the reader last looked names nothing.
    #[test]
    fn a_request_maps_back_to_the_control_that_claimed_it() {
        let nodes = vec![node(Some(7), Role::Button, "Save")];
        let update = tree_update(&nodes, "Editor");
        let target = update.nodes[0].0;

        let request = ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: None,
        };
        assert_eq!(requested_focus_id(&request, &nodes), Some((7, true)));

        let stale = ActionRequest {
            action: Action::Focus,
            target_tree: TreeId::ROOT,
            target_node: NodeId(target.0 + 1),
            data: None,
        };
        assert_eq!(requested_focus_id(&stale, &nodes), None);
        assert_eq!(
            requested_focus_id(
                &ActionRequest {
                    action: Action::Focus,
                    target_tree: TreeId::ROOT,
                    target_node: ROOT,
                    data: None
                },
                &nodes
            ),
            None,
            "the window itself is not a control"
        );
    }
}
