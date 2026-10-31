use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Color;
use layout_core::{AvailableSpace, LayoutStyle};
use renderer_core::{DrawCommand, RectStyle, ShapeStyle};
use ui_tree::ComponentList;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, live_node_count, reset_layout_runtime};
use crate::layout_item::box_item;
use crate::rect::Rectangle;

#[derive(Clone)]
struct Menu(&'static str);

/// A child records what it could see of its parent while it was being built.
fn spy(seen: Rc<RefCell<Vec<Option<&'static str>>>>) -> Children {
    Children::new(move || {
        seen.borrow_mut().push(use_context::<Menu>().map(|m| m.0));
        Ok(vec![box_item(Container::new(LayoutStyle::new(), vec![])?)])
    })
}

/// The inversion the whole type exists for. A child is an argument, so it is normally constructed before its parent — build it from a recipe instead and the parent gets to exist first.
#[test]
fn a_child_built_from_the_recipe_can_see_the_parent_making_it() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let children = spy(seen.clone());

    let built = children.build_with(Menu("edit")).unwrap();
    assert_eq!(*seen.borrow(), vec![Some("edit")]);
    assert_eq!(built.len(), 1, "and it is still a child, not just a reader");
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
            Ok(Vec::new())
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

/// Children built where they are placed see what the owner around the placement provides, which is what a `theme:` or a context on the node holding a `children` placeholder relies on.
#[test]
fn children_see_what_is_provided_where_they_are_placed() {
    reset_layout_runtime();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let children = spy(seen.clone());
    {
        let _placement = reactive_core::owner_scope();
        services_core::provide(Menu("placed")).unwrap();
        children.build().unwrap();
    }
    children.build().unwrap();
    assert_eq!(*seen.borrow(), vec![Some("placed"), None]);
}

const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);

#[derive(Clone)]
struct Shade(Color);

/// The context is read when the children draw, long after the build that provided it returned, not only while they were being made.
#[test]
fn a_context_stays_in_force_when_its_children_draw() {
    reset_layout_runtime();
    let children = Children::new(|| {
        let swatch = Rectangle::new(LayoutStyle::new().width(10.0).height(10.0), || {
            RectStyle::default().with_fill(use_context::<Shade>().map_or(Color::BLACK, |s| s.0))
        })?;
        Ok(vec![box_item(swatch)])
    });
    let row = Container::new(
        LayoutStyle::new().width(100.0).height(20.0),
        children.build_with(Shade(RED)).unwrap(),
    )
    .unwrap();
    compute_layout(
        row.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();
    let tree = ComponentList::new(row);
    let fills: Vec<Color> = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.fill.as_ref().map(|p| p.solid_color()),
            _ => None,
        })
        .collect();
    assert_eq!(fills, vec![RED]);
}

/// A component whose caller handed it no children holds the default recipe, which builds nothing.
#[test]
fn the_default_children_build_nothing() {
    reset_layout_runtime();
    let before = live_node_count();
    assert!(Children::default().build().unwrap().is_empty());
    assert!(
        Children::default()
            .build_with(Menu("edit"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(live_node_count(), before);
}

/// A recipe for one item builds exactly one item, and again on every build.
#[test]
fn one_builds_a_single_item_each_time_it_is_asked() {
    reset_layout_runtime();
    let children = Children::one(|| Ok(box_item(Container::new(LayoutStyle::new(), vec![])?)));
    assert_eq!(children.build().unwrap().len(), 1);
    assert_eq!(children.build().unwrap().len(), 1);
}
