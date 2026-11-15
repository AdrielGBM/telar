use telar::{
    AccessNode, AvailableSpace, ComponentList, Container, DrawCommand, NodeId, RwSignal, Text,
    compute_layout, focus, signal,
};

use super::*;
use crate::{ButtonProps, TabsProps, button, tabs};

/// A row of two tabs over the panel they share, which shows a line of text and a button for the chosen tab.
fn page(selected: RwSignal<usize>) -> (ComponentList, NodeId) {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let link = TabPanelLink::new();
    let row = tabs(
        TabsProps::props()
            .items(vec!["Overview", "Pricing"])
            .selected(selected)
            .panel(link.clone())
            .build(),
    )
    .unwrap();
    let body = Children::one(move || {
        let text = Text::declaring(
            move || format!("About the {}", ["product", "plans"][selected.get()]),
            LayoutStyle::new(),
            |t| t,
        )?;
        let more = button(ButtonProps::props().label("More").build())?;
        Ok(box_item(Container::new(
            LayoutStyle::column(),
            vec![box_item(text), more],
        )?))
    });
    let panel = tab_panel(TabPanelProps::props().link(link).children(body).build()).unwrap();
    let panel_node = panel.layout_node();
    let root = Container::new(
        LayoutStyle::column().width(400.0).height(300.0),
        vec![row, panel],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();
    (ComponentList::new(box_item(root)), panel_node)
}

fn read(tree: &ComponentList) -> Vec<AccessNode> {
    telar::relayout_if_dirty();
    telar::inspect::snapshot(&tree.commands())
}

fn named<'a>(nodes: &'a [AccessNode], role: Role, name: &str) -> &'a AccessNode {
    nodes
        .iter()
        .find(|node| node.role == role && node.name == name)
        .unwrap_or_else(|| panic!("a {role:?} named {name}: {nodes:?}"))
}

fn panel_of(nodes: &[AccessNode]) -> &AccessNode {
    nodes
        .iter()
        .find(|node| node.role == Role::TabPanel)
        .unwrap_or_else(|| panic!("a tab panel: {nodes:?}"))
}

#[test]
fn the_chosen_tab_controls_the_panel_and_the_panel_is_called_by_it() {
    let selected = signal(0usize);
    let (tree, _) = page(selected);
    let nodes = read(&tree);
    let panel = panel_of(&nodes);
    assert_eq!(panel.name, "Overview");
    assert!(panel.id.is_some(), "the panel is a node a tab can point at");
    assert_eq!(named(&nodes, Role::Tab, "Overview").controls, panel.id);
    assert_eq!(
        named(&nodes, Role::Tab, "Pricing").controls,
        None,
        "a tab not chosen shows no panel"
    );

    selected.set(1);
    let nodes = read(&tree);
    let panel = panel_of(&nodes);
    assert_eq!(panel.name, "Pricing");
    assert_eq!(named(&nodes, Role::Tab, "Pricing").controls, panel.id);
    assert_eq!(named(&nodes, Role::Tab, "Overview").controls, None);
}

/// A document points the chosen tab at the panel's element by the box it is.
#[test]
fn a_document_is_told_the_chosen_tab_controls_the_panel_s_box() {
    let was = telar::testing::set_element_capture(true);
    let (tree, panel) = page(signal(0usize));
    let controls: Vec<Option<u64>> = tree
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::PushElement { element } if element.semantics.role == Role::Tab => {
                Some(element.semantics.controls)
            }
            _ => None,
        })
        .collect();
    telar::testing::set_element_capture(was);
    assert_eq!(controls, [Some(u64::from(panel)), None]);
}

#[test]
fn what_the_panel_shows_is_read_inside_it() {
    let (tree, _) = page(signal(1usize));
    let nodes = read(&tree);
    let at = nodes
        .iter()
        .position(|node| node.role == Role::TabPanel)
        .expect("a tab panel");
    let text = nodes
        .iter()
        .find(|node| node.name == "About the plans")
        .unwrap_or_else(|| panic!("the panel's text is read: {nodes:?}"));
    assert_eq!(text.parent, Some(at));
    assert_eq!(named(&nodes, Role::Button, "More").parent, Some(at));
}

/// The panel is no stop of its own: Tab goes from the chosen tab to the first control inside the panel.
#[test]
fn tab_goes_from_the_chosen_tab_into_the_panel_s_content() {
    let (tree, _) = page(signal(1usize));
    let nodes = read(&tree);
    focus::focus_next();
    assert_eq!(focus::current(), named(&nodes, Role::Tab, "Pricing").id);
    focus::focus_next();
    assert_eq!(focus::current(), named(&nodes, Role::Button, "More").id);
    focus::clear();
}

/// A reader moving to the panel takes the keyboard to the first control inside it.
#[test]
fn a_reader_moving_to_the_panel_reaches_its_first_control() {
    let (tree, _) = page(signal(0usize));
    let nodes = read(&tree);
    let panel = panel_of(&nodes).id.expect("the panel has an id");
    assert!(focus::perform(panel, focus::AccessAction::Focus));
    assert_eq!(focus::current(), named(&nodes, Role::Button, "More").id);
    focus::clear();
}

/// A press on the panel where nothing in it takes one lets go of the keyboard, as a press anywhere else empty does.
#[test]
fn a_press_on_the_panel_s_empty_room_takes_the_keyboard_away() {
    let (tree, panel) = page(signal(0usize));
    let nodes = read(&tree);
    focus::request(named(&nodes, Role::Tab, "Overview").id.unwrap());
    let room = telar::track_layout(panel).unwrap().get();
    focus::blur_from_pointer(room.x + room.width - 4.0, room.y + room.height - 4.0);
    assert_eq!(focus::current(), None);
}
