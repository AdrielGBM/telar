use super::*;
use crate::Rect;

fn node(role: Role, name: &str) -> AccessNode {
    AccessNode {
        id: Some(1),
        role,
        name: name.to_string(),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        lang: None,
        url: None,
        current: None,
        orientation: None,
        expanded: None,
        position: None,
        active_descendant: None,
    }
}

#[test]
fn a_role_query_names_the_whole_name_ignoring_the_space_around_it() {
    let query = by_role(Role::Button).named("Save");
    assert!(query.matches(&node(Role::Button, " Save ")));
    assert!(!query.matches(&node(Role::Button, "Save all")));
    assert!(!query.matches(&node(Role::Link, "Save")));
}

#[test]
fn a_text_query_matches_any_role() {
    let query = by_text("Saved");
    assert!(query.matches(&node(Role::Label, "Saved")));
    assert!(query.matches(&node(Role::Status, "Saved")));
}

#[test]
fn containing_matches_a_part_of_the_name() {
    let query = by_role(Role::Label).containing("3 results");
    assert!(query.matches(&node(Role::Label, "Showing 3 results")));
}

#[test]
fn nth_picks_one_match_in_reading_order() {
    let snapshot = [
        node(Role::Button, "A"),
        node(Role::Label, "x"),
        node(Role::Button, "B"),
    ];
    let second = by_role(Role::Button).nth(1).find_all(&snapshot);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].name, "B");
    assert!(by_role(Role::Button).nth(2).find_all(&snapshot).is_empty());
}

#[test]
fn a_query_reads_as_what_it_looks_for() {
    assert_eq!(
        by_role(Role::Button).named("Save").to_string(),
        "button \"Save\""
    );
    assert_eq!(by_text("Hi").nth(2).to_string(), "node \"Hi\" #2");
}
