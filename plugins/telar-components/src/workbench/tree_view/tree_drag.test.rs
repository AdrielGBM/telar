use std::collections::HashSet;

use super::*;

/// `inputs` holding `button` (holding `primary` and `ghost`) and `checkbox`, then `layout` holding `stack`, then `about`, with `inputs` and `button` open.
fn shown() -> Rc<Vec<Rc<Row<&'static str>>>> {
    let items = vec![
        TreeNode::new("inputs", "Inputs").with_children([
            TreeNode::new("button", "Button").with_children([
                TreeNode::new("primary", "Primary"),
                TreeNode::new("ghost", "Ghost"),
            ]),
            TreeNode::new("checkbox", "Checkbox"),
        ]),
        TreeNode::new("layout", "Layout").with_children([TreeNode::new("stack", "Stack")]),
        TreeNode::new("about", "About"),
    ];
    let open: HashSet<&str> = ["inputs", "button"].into_iter().collect();
    Flat::of(&items, &open).0
}

fn between(
    parent: Option<&'static str>,
    after: Option<&'static str>,
    before: Option<&'static str>,
) -> Option<TreeTarget<&'static str>> {
    Some(TreeTarget::Between {
        parent,
        after,
        before,
    })
}

#[test]
fn the_gap_before_a_row_is_among_its_siblings_after_the_one_before_it() {
    let rows = shown();
    assert_eq!(
        target_of(&rows, DropPlace::Between(0)),
        between(None, None, Some("inputs"))
    );
    assert_eq!(
        target_of(&rows, DropPlace::Between(2)),
        between(Some("button"), None, Some("primary")),
        "the first gap inside an open branch"
    );
    assert_eq!(
        target_of(&rows, DropPlace::Between(4)),
        between(Some("inputs"), Some("button"), Some("checkbox")),
        "after a sibling, passing over what is shown under it"
    );
    assert_eq!(
        target_of(&rows, DropPlace::Between(5)),
        between(None, Some("inputs"), Some("layout"))
    );
}

#[test]
fn the_gap_after_the_last_row_is_the_end_of_the_top_level() {
    let rows = shown();
    assert_eq!(
        target_of(&rows, DropPlace::Between(rows.len())),
        between(None, Some("about"), None)
    );
}

#[test]
fn onto_a_row_is_into_its_node() {
    let rows = shown();
    assert_eq!(
        target_of(&rows, DropPlace::Inside(1)),
        Some(TreeTarget::Inside("button"))
    );
    assert_eq!(target_of(&rows, DropPlace::Inside(40)), None);
}

#[test]
fn a_node_never_lands_in_itself_or_under_it() {
    let rows = shown();
    assert!(lands_in_itself(&rows, &"inputs", &TreeTarget::Inside("button")));
    assert!(lands_in_itself(
        &rows,
        &"button",
        &TreeTarget::Between {
            parent: Some("button"),
            after: None,
            before: Some("primary")
        }
    ));
    assert!(!lands_in_itself(&rows, &"button", &TreeTarget::Inside("layout")));
    assert!(!lands_in_itself(
        &rows,
        &"button",
        &TreeTarget::Between {
            parent: Some("inputs"),
            after: Some("checkbox"),
            before: None
        }
    ));
}

#[test]
fn either_gap_beside_a_node_is_where_it_already_stands() {
    assert!(puts_back(&"checkbox", &between(Some("inputs"), Some("button"), Some("checkbox")).unwrap()));
    assert!(puts_back(&"button", &between(Some("inputs"), Some("button"), Some("checkbox")).unwrap()));
    assert!(!puts_back(&"about", &between(Some("inputs"), Some("button"), Some("checkbox")).unwrap()));
}
