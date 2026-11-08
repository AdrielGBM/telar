use telar::{AccessNode, ComponentList, Role};

pub fn center(node: &AccessNode) -> (f64, f64) {
    (
        f64::from(node.rect.x + node.rect.width / 2.0),
        f64::from(node.rect.y + node.rect.height / 2.0),
    )
}

fn has_ancestor_with_role(nodes: &[AccessNode], mut at: Option<usize>, role: Role) -> bool {
    while let Some(index) = at {
        if nodes[index].role == role {
            return true;
        }
        at = nodes[index].parent;
    }
    false
}

/// The centre of the contents-rail button titled `title`, in window coordinates, read from the access snapshot so a press lands wherever layout puts the button.
pub fn nav_button_center(tree: &ComponentList, title: &str) -> (f64, f64) {
    let nodes = telar::inspect::snapshot(&tree.commands());
    let button = nodes
        .iter()
        .find(|node| {
            node.role == Role::Button
                && node.name == title
                && has_ancestor_with_role(&nodes, node.parent, Role::Navigation)
        })
        .unwrap_or_else(|| panic!("no nav button titled {title:?} in the contents rail"));
    center(button)
}
