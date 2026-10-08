use telar::{AvailableSpace, ComponentList, DrawCommand, compute_layout, reset_layout_runtime};

use super::*;

#[test]
fn the_icon_named_from_rust_is_baked_and_draws() {
    reset_layout_runtime();
    let item = marker(Children::default()).unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    let commands = tree.commands();
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Path { .. })),
        "{commands:?}"
    );
}
