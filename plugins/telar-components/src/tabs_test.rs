use telar::{
    AvailableSpace, Component, ComponentList, DrawCommand, NodeId, compute_layout, new_container,
};

use super::*;
use crate::harness::{press, release};

/// A bar in a region that declared its own ink writes the tabs that are not selected in a quieter shade of *that*, rather than in a grey no theme and no declaration can move.
#[test]
fn an_inactive_tab_fades_the_ink_of_the_region_around_it() {
    crate::test_support::fresh_layout_runtime();
    let item = tabs(
        TabsProps::props().items(vec!["One", "Two"]).build(),
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(100.0),
        &[item.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    telar::declare(root, telar::Declared::default().with_color(declared));
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let tree = ComponentList::new(item);
    let ink = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Two" => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .expect("the bar drew the tab that is not selected");
    assert_eq!(ink, declared.with_alpha(shared::QUIET_ALPHA));
}

// As the sole child of a 400×100 root: an auto-size root fills its available space, so laying `node` out as the root would force it to 400px wide instead of its natural content width.
fn lay_out(node: NodeId) -> telar::Rect {
    crate::harness::lay_out_row(node, 400.0, 100.0)
}

#[test]
fn builds_and_lays_out() {
    crate::test_support::fresh_layout_runtime();
    let item = tabs(
        TabsProps::props()
            .items(vec!["One", "Two", "Three"])
            .build(),
        Children::default(),
    )
    .unwrap();
    let r = lay_out(item.layout_node());
    assert!(r.width > 0.0, "the tab row takes some width");
    let _ = item.view();
}

// With two content-sized tabs and no `flex_grow`, the row's right edge sits flush against the second tab's padding, so a point just inside it lands on index 1 without hardcoding font metrics.
#[test]
fn pressing_the_last_tab_sets_selected_index() {
    crate::test_support::fresh_layout_runtime();
    let selected = signal(0u32);
    let mut item = tabs(
        TabsProps::props()
            .items(vec!["One", "Two"])
            .selected(selected)
            .build(),
        Children::default(),
    )
    .unwrap();
    let r = lay_out(item.layout_node());
    let (cx, cy) = ((r.x + r.width - 2.0) as f64, (r.y + r.height / 2.0) as f64);

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));

    assert_eq!(
        selected.get(),
        1,
        "pressing the last (second) tab sets selected to its index"
    );
}

/// The bar is the tab list its tabs belong to, named when the caller names it, and naming it does not rename a tab.
#[test]
fn the_bar_is_a_tab_list_a_reader_can_name() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let item = tabs(
        TabsProps::props()
            .items(vec!["General", "Advanced"])
            .label("Settings sections")
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(item.layout_node());
    let tree = ComponentList::new(item);
    let nodes = ui_core::accessibility::snapshot(&tree.commands());

    let mut tabs: Vec<_> = nodes
        .iter()
        .filter(|node| node.role == Role::Tab)
        .map(|node| node.name.as_str())
        .collect();
    tabs.sort();
    assert_eq!(tabs, ["Advanced", "General"]);
    assert!(
        nodes
            .iter()
            .any(|node| node.id.is_none() && node.name == "Settings sections"),
        "the list's name is read: {nodes:?}"
    );
}
