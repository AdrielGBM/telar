use std::cell::Cell;

use telar::testing::{centre, lay_out, named, press, release, route};
use telar::{
    AccessNode, ComponentList, DrawCommand, Location, Role, anchor, external, focus,
    relayout_if_dirty, signal,
};

use super::*;

fn mounted(props: LinkProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let item = link(props, Children::default()).unwrap();
    lay_out(item.layout_node(), 400.0, 100.0);
    let tree = ComponentList::new(item);
    let _ = tree.commands();
    tree
}

fn the_link(tree: &ComponentList) -> AccessNode {
    ui_core::accessibility::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::Link)
        .expect("a link")
}

fn drawn_style(tree: &ComponentList, text: &str) -> telar::TextStyle {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { text: t, style, .. } if t.as_ref() == text => {
                Some((**style).clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{text}` is drawn"))
}

fn draws_a_mark(tree: &ComponentList) -> bool {
    tree.commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::Path { .. }))
}

#[test]
fn a_link_is_a_link_named_by_its_words_and_told_apart_by_more_than_colour() {
    let tree = mounted(
        LinkProps::props()
            .label("Guide")
            .to(Location::root().segment("guide"))
            .build(),
    );
    let node = the_link(&tree);
    assert_eq!(node.name, "Guide");
    assert!(node.url.is_some(), "a reader is told where it goes");
    let style = drawn_style(&tree, "Guide");
    assert_eq!(style.decoration.line, telar::DecorationLine::Underline);
    let ink = style.color.solid_color();
    assert!(
        shared::surface().contrast_ratio(ink) >= 4.5,
        "the accent is taken toward the ink until it reads on the surface"
    );
    assert!(
        !draws_a_mark(&tree),
        "a link inside the application draws no arrow"
    );
}

#[test]
fn a_link_that_leaves_the_application_says_so_to_the_eye_and_the_ear() {
    let tree = mounted(
        LinkProps::props()
            .label("Source")
            .to(external("https://example.com/telar"))
            .build(),
    );
    assert_eq!(the_link(&tree).name, "Source (external link)");
    assert!(draws_a_mark(&tree), "an arrow follows the words");
}

#[test]
fn the_arrow_follows_where_the_link_goes() {
    let leaves = signal(false);
    let tree = mounted(
        LinkProps::props()
            .label("Docs")
            .to(LinkTarget::of(move || match leaves.get() {
                true => external("https://example.com/docs").ok(),
                false => Some(anchor("docs")),
            }))
            .build(),
    );
    assert!(!draws_a_mark(&tree));
    leaves.set(true);
    relayout_if_dirty();
    assert!(draws_a_mark(&tree));
    assert_eq!(the_link(&tree).name, "Docs (external link)");
}

#[test]
fn the_current_link_is_announced_as_current_and_drawn_strong() {
    let tree = mounted(
        LinkProps::props()
            .label("Home")
            .to(Location::root())
            .current(true)
            .build(),
    );
    assert!(the_link(&tree).current.is_some());
    assert_eq!(
        drawn_style(&tree, "Home").font_weight,
        shared::strong_weight()
    );
}

#[test]
fn a_press_runs_its_handler_and_the_keyboard_reaches_it() {
    let pressed = Rc::new(Cell::new(0));
    let sink = pressed.clone();
    let mut tree = mounted(
        LinkProps::props()
            .label("Settings")
            .to(anchor("settings"))
            .on_press(Rc::new(move || sink.set(sink.get() + 1)))
            .build(),
    );
    let at = centre(the_link(&tree).rect);
    route(&mut tree, &press(at.0, at.1));
    route(&mut tree, &release(at.0, at.1));
    assert_eq!(pressed.get(), 1);

    focus::clear();
    focus::focus_next();
    assert_eq!(focus::current(), the_link(&tree).id, "Tab reaches it");
    route(&mut tree, &named(telar::NamedKey::Enter));
    assert_eq!(pressed.get(), 2, "and Enter presses it");
}

#[test]
fn a_link_with_nowhere_to_go_points_a_reader_nowhere() {
    let tree = mounted(
        LinkProps::props()
            .label("Profile")
            .to(LinkTarget::of(|| None::<Location>))
            .build(),
    );
    assert_eq!(the_link(&tree).url, None);
}
