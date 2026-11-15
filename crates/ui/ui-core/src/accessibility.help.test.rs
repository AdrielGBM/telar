use layout_core::{AvailableSpace, LayoutStyle};
use reactive_core::signal;
use renderer_core::{Element, RectStyle};
use ui_tree::{Component, ComponentList, RenderNode};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;

fn control() -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(160.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::TextInput)
}

fn mounted(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = Container::new(LayoutStyle::new().flex_column(), vec![item]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    ComponentList::new(root)
}

fn the_field(tree: &ComponentList) -> AccessNode {
    snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::TextInput)
        .expect("the field is read")
}

fn elements(view: &RenderNode) -> Vec<Element> {
    let mut out = Vec::new();
    collect(view, &mut out);
    out
}

fn collect(node: &RenderNode, out: &mut Vec<Element>) {
    match node {
        RenderNode::Element { element, children } => {
            out.push((**element).clone());
            children.iter().for_each(|child| collect(child, out));
        }
        RenderNode::Group { children }
        | RenderNode::Transform { children, .. }
        | RenderNode::Clip { children, .. }
        | RenderNode::Layer { children, .. }
        | RenderNode::Overlay { children } => children.iter().for_each(|child| collect(child, out)),
        _ => {}
    }
}

/// What a field asks of what is typed into it, and the help that goes with it, reaches a reader walking the surface.
#[test]
fn a_field_reports_its_help_and_what_it_asks_to_a_reader() {
    reset_layout_runtime();
    focus::clear();
    let field = control()
        .described(|| Some("At least 8 characters".to_string()))
        .invalid(|| true)
        .required(|| true)
        .readonly(|| true);
    let tree = mounted(box_item(field));

    let node = the_field(&tree);
    assert_eq!(node.description.as_deref(), Some("At least 8 characters"));
    assert!(node.invalid && node.required && node.readonly);
}

/// The element a document makes of the field carries the same answers, read from the same registry.
#[test]
fn a_field_carries_its_help_and_what_it_asks_onto_its_element() {
    reset_layout_runtime();
    focus::clear();
    let field = control()
        .described(|| Some("At least 8 characters".to_string()))
        .invalid(|| true)
        .required(|| true);

    let was = ui_tree::set_element_capture(true);
    let found = elements(&field.view());
    ui_tree::set_element_capture(was);

    let element = found
        .iter()
        .find(|element| element.semantics.role == Role::TextInput)
        .expect("the field is an element");
    assert_eq!(
        element.semantics.description.as_deref(),
        Some("At least 8 characters")
    );
    assert!(element.semantics.invalid && element.semantics.required);
    assert!(
        !element.semantics.readonly,
        "it said nothing of being read only"
    );
}

/// A message that appears as the person types is heard without the field being rebuilt, and one that goes away says nothing.
#[test]
fn a_description_follows_what_it_reads() {
    reset_layout_runtime();
    focus::clear();
    let message = signal(None::<String>);
    let field = control()
        .described(move || message.get())
        .invalid(move || message.with(Option::is_some));
    let tree = mounted(box_item(field));

    let quiet = the_field(&tree);
    assert_eq!(quiet.description, None);
    assert!(!quiet.invalid);

    message.set(Some("Enter an address with an @".to_string()));
    let wrong = the_field(&tree);
    assert_eq!(
        wrong.description.as_deref(),
        Some("Enter an address with an @")
    );
    assert!(wrong.invalid);

    message.set(None);
    let fixed = the_field(&tree);
    assert_eq!(fixed.description, None);
    assert!(!fixed.invalid);
}

/// A control that declared nothing is not announced as invalid, required or read only.
#[test]
fn a_control_that_says_nothing_is_read_as_plain() {
    reset_layout_runtime();
    focus::clear();
    let tree = mounted(box_item(control()));

    let node = the_field(&tree);
    assert_eq!(node.description, None);
    assert!(!node.invalid && !node.required && !node.readonly);
}
