use layout_core::LayoutStyle;
use reactive_core::{dispose_owner, owner_scope, signal};
use renderer_core::{Color, Element, TextStyle};
use ui_tree::{Component, RenderNode};

use super::*;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;
use crate::text::Text;

fn letter(c: &'static str) -> Text {
    Text::new(
        move || c.to_string(),
        LayoutStyle::new(),
        || TextStyle::new(12.0, Color::BLACK),
    )
    .unwrap()
}

fn captured(view: &RenderNode) -> Vec<Element> {
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

fn as_document<R>(f: impl FnOnce() -> R) -> R {
    let was = ui_tree::set_element_capture(true);
    let out = f();
    ui_tree::set_element_capture(was);
    out
}

/// The case the three annotations exist for: a word drawn one letter per box reads as the word, once, and a document says so with the attributes a reader understands.
#[test]
fn split_letters_reach_a_document_as_one_named_word() {
    reset_layout_runtime();
    let h = letter("H").a11y_hidden();
    let i = letter("i").a11y_hidden();
    let letters = as_document(|| [captured(&h.view()), captured(&i.view())]);
    let word = Container::new(LayoutStyle::new(), vec![box_item(h), box_item(i)])
        .unwrap()
        .a11y_label(|| "Hi")
        .a11y_lang(|| "en");
    let row = as_document(|| captured(&word.view()));
    assert_eq!(row[0].semantics.label.as_deref(), Some("Hi"));
    assert_eq!(row[0].semantics.lang.as_deref(), Some("en"));
    assert!(!row[0].semantics.hidden, "the word itself is read");
    assert!(
        letters.iter().all(|letter| letter[0].semantics.hidden),
        "and none of its letters is"
    );
}

/// A box nobody annotated is exactly the element it always was.
#[test]
fn an_unannotated_box_says_nothing_new() {
    reset_layout_runtime();
    let text = letter("x");
    let elements = as_document(|| captured(&text.view()));
    assert!(elements[0].semantics.label.is_none());
    assert!(elements[0].semantics.lang.is_none());
    assert!(!elements[0].semantics.hidden);
}

/// A name built from state follows it, the way a translated name follows the locale.
#[test]
fn a_label_follows_what_it_reads() {
    reset_layout_runtime();
    let name = signal(String::from("Play"));
    let button = letter("▶").a11y_label(move || name.get());
    let node = button.layout_node();
    assert_eq!(peek(node).and_then(|a| a.label).as_deref(), Some("Play"));
    name.set(String::from("Pause"));
    assert_eq!(peek(node).and_then(|a| a.label).as_deref(), Some("Pause"));
}

/// Withdrawn with the scope that said it, so a node id minted again later does not inherit a stranger's name.
#[test]
fn an_annotation_goes_with_the_scope_that_made_it() {
    reset_layout_runtime();
    let scope = owner_scope();
    let text = letter("x").a11y_label(|| "gone soon");
    let node = text.layout_node();
    let id = scope.id();
    drop(scope);
    assert!(peek(node).is_some());
    dispose_owner(id);
    assert!(peek(node).is_none());
}
