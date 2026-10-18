use telar::memo;

use super::*;

fn tree() -> Vec<TreeNode> {
    vec![
        TreeNode::new("page", "Page").with_children([
            TreeNode::new("form", "Form").with_children([TreeNode::new("save", "Save")]),
            TreeNode::new("note", "Note"),
        ]),
        TreeNode::new("footer", "Footer").with_children([TreeNode::new("link", "Link")]),
    ]
}

fn open(expanded: RwSignal<HashSet<Arc<str>>>) -> Vec<String> {
    let mut ids: Vec<String> = expanded.with(|open| open.iter().map(|id| id.to_string()).collect());
    ids.sort();
    ids
}

#[test]
fn a_branch_offers_its_own_commands_before_the_whole_tree_s() {
    let nodes = tree();
    assert_eq!(
        commands(Some(&nodes[0])),
        [
            Some(Command::Expand),
            Some(Command::Collapse),
            None,
            Some(Command::ExpandAll),
            Some(Command::CollapseAll)
        ]
    );
    let leaf = &nodes[0].children[1];
    assert_eq!(
        commands(Some(leaf)),
        [Some(Command::ExpandAll), Some(Command::CollapseAll)]
    );
    assert_eq!(
        commands(None),
        [Some(Command::ExpandAll), Some(Command::CollapseAll)],
        "the space under the rows"
    );
}

#[test]
fn a_branch_opens_and_shuts_with_everything_under_it_and_the_tree_all_at_once() {
    let items = memo(tree);
    let expanded = signal(HashSet::new());
    let menu = TreeMenu::new(items, expanded);
    let nodes = tree();

    menu.run(Command::Expand, Some(&nodes[0]));
    assert_eq!(open(expanded), ["form", "page"]);

    menu.run(Command::ExpandAll, None);
    assert_eq!(open(expanded), ["footer", "form", "page"]);

    menu.run(Command::Collapse, Some(&nodes[0]));
    assert_eq!(open(expanded), ["footer"], "the other branch stays open");

    menu.run(Command::CollapseAll, None);
    assert!(open(expanded).is_empty());
}
