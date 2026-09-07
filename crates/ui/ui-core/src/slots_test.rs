use std::cell::RefCell;
use std::rc::Rc;

use layout_core::LayoutStyle;

use super::*;
use crate::container::Container;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;

#[derive(Clone)]
struct Menu(&'static str);

/// A child records what it could see of its parent while it was being built.
fn spy(seen: Rc<RefCell<Vec<Option<&'static str>>>>) -> Children {
    Children::new(move || {
        seen.borrow_mut().push(use_context::<Menu>().map(|m| m.0));
        let mut slots = Slots::new();
        slots.push(None, box_item(Container::new(LayoutStyle::new(), vec![])?));
        Ok(slots)
    })
}

/// The inversion the whole type exists for. A child is an argument, so it is normally constructed before its parent — build it from a recipe instead and the parent gets to exist first.
#[test]
fn a_child_built_from_the_recipe_can_see_the_parent_making_it() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let children = spy(seen.clone());

    let slots = children.build_with(Menu("edit")).unwrap();
    assert_eq!(*seen.borrow(), vec![Some("edit")]);
    assert_eq!(slots.len(), 1, "and it is still a child, not just a reader");
}

/// A piece used on its own gets `None` rather than a panic: the mistake was made in markup, and a component that dies on it reports it as a crash in Rust nobody wrote.
#[test]
fn a_child_outside_any_parent_sees_nothing() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    spy(seen.clone()).build().unwrap();
    assert_eq!(*seen.borrow(), vec![None]);
}

/// The context closes with the build. A widget made afterwards is not inside that menu, and must not find it lying around.
#[test]
fn the_context_does_not_outlive_the_build_that_opened_it() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let children = spy(seen.clone());
    children.build_with(Menu("edit")).unwrap();
    assert_eq!(use_context::<Menu>().map(|m| m.0), None);
}

/// Nesting is what a submenu is, and the inner one has to win inside itself without disturbing the outer.
#[test]
fn a_nested_parent_shadows_the_one_it_sits_in() {
    reset_layout_runtime();
    let inner_seen = Rc::new(RefCell::new(Vec::new()));
    let inner = spy(inner_seen.clone());
    let outer_seen = Rc::new(RefCell::new(Vec::new()));
    let outer = {
        let outer_seen = outer_seen.clone();
        Children::new(move || {
            outer_seen
                .borrow_mut()
                .push(use_context::<Menu>().map(|m| m.0));
            inner.build_with(Menu("submenu"))?;
            outer_seen
                .borrow_mut()
                .push(use_context::<Menu>().map(|m| m.0));
            Ok(Slots::new())
        })
    };

    outer.build_with(Menu("edit")).unwrap();
    assert_eq!(*inner_seen.borrow(), vec![Some("submenu")]);
    assert_eq!(*outer_seen.borrow(), vec![Some("edit"), Some("edit")]);
}

/// The reason the recipe is `Fn` and not `FnOnce`: a dropdown throws its rows away and remakes them every time the panel opens, so children that could only be built once would come back empty on the second open.
#[test]
fn the_recipe_can_be_run_more_than_once() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let children = spy(seen.clone());

    for _ in 0..3 {
        assert_eq!(children.build_with(Menu("edit")).unwrap().len(), 1);
    }
    assert_eq!(seen.borrow().len(), 3, "a fresh set of rows each time");
}
