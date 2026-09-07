use layout_core::AvailableSpace;
use renderer_core::DrawCommand;
use ui_core::{ComponentList, compute_layout, new_container};

use super::*;

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

fn laid_out(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
    ComponentList::new(item)
}

#[test]
fn renders_label() {
    crate::test_support::fresh_layout_runtime();
    let badge = badge(
        BadgeProps::props().label("New").build(),
        Children::default(),
    )
    .unwrap();
    let tree = laid_out(badge);
    assert!(
        find_text(&tree.commands(), "New"),
        "a badge draws its label"
    );
}

#[test]
fn empty_label_builds_without_panic() {
    crate::test_support::fresh_layout_runtime();
    let badge = badge(BadgeProps::props().build(), Children::default()).unwrap();
    let tree = laid_out(badge);
    let _ = tree.commands();
}
