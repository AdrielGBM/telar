use ui_core::NodeId;

use super::*;

fn lay_out(node: NodeId) {
    crate::harness::lay_out(node, 100.0, 100.0);
}

#[test]
fn spinner_builds_with_default_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(SpinnerProps::props().build(), Children::default());
    assert!(widget.is_ok(), "a spinner builds from its default size");
    lay_out(widget.unwrap().layout_node());
}

#[test]
fn spinner_builds_with_custom_size() {
    crate::test_support::fresh_layout_runtime();
    let widget = spinner(
        SpinnerProps::props().size(48.0).build(),
        Children::default(),
    )
    .unwrap();
    lay_out(widget.layout_node());
}
