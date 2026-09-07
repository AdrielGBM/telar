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

/// The shape a screen reader is handed: one window, every control and every piece of text under it, and each control addressable so focus and activation can come back.
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

/// A control keeps its identity across frames, so a reader is not told the button appeared anew every time anything else on screen moved. The label beside it has no identity to keep and does not need one.
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

/// A request coming back names a node; only a control has a focus id behind it, and one that has gone from the tree since the reader last looked names nothing.
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
