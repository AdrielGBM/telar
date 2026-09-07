use super::*;
use layout_core::AvailableSpace;
use reactive_core::signal;
use renderer_core::DrawCommand;
use ui_core::{ComponentList, Text, compute_layout, new_container, relayout_if_dirty};

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

fn slot_with_body(label: &'static str) -> Slots {
    let body = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new().height(20.0),
        |t| t,
    )
    .unwrap();
    let mut slots = Slots::new();
    slots.push(None, box_item(body));
    slots
}

#[test]
fn open_shows_panel_and_close_hides_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let slots = slot_with_body("Drawer body");
    let drawer = drawer(
        DrawerProps::props().open(open).side("right").build(),
        Children::from(slots),
    )
    .unwrap();

    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[drawer.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(drawer);

    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "closing hides the panel again"
    );

    open.set(true);
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Drawer body"),
        "body shows when open"
    );

    open.set(false);
    relayout_if_dirty();
    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "body hidden when closed"
    );
}

#[test]
fn unbound_drawer_renders_nothing() {
    crate::test_support::fresh_layout_runtime();
    let slots = slot_with_body("Drawer body");
    let drawer = drawer(DrawerProps::props().build(), Children::from(slots)).unwrap();
    let tree = ComponentList::new(drawer);
    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "a drawer bound to nothing draws nothing"
    );
}
