use super::*;
use crate::preview::PreviewCtx;
use crate::{Container, LayoutError, LayoutItem, LayoutStyle};

fn empty(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(LayoutStyle::new(), Vec::new())?))
}

#[test]
fn distinct_ids_are_fine() {
    let entries = [
        PreviewEntry::new("demo--card--a", "card", "A", empty),
        PreviewEntry::new("demo--card--b", "card", "B", empty),
    ];
    assert_eq!(duplicate_ids(&entries), None);
}

#[test]
fn a_repeated_id_names_where_each_of_its_previews_is_written() {
    let entries = [
        PreviewEntry::new("demo--card--a", "card", "A", empty).location("/src/a.rs", 3),
        PreviewEntry::new("demo--card--b", "card", "B", empty),
        PreviewEntry::new("demo--card--a", "card", "a", empty).location("/src/b.rs", 9),
    ];
    assert_eq!(
        duplicate_ids(&entries).as_deref(),
        Some(
            "preview id `demo--card--a` names 2 previews (/src/a.rs:3, /src/b.rs:9); rename all but one"
        )
    );
}

#[test]
fn a_preview_with_no_file_is_named_instead() {
    let entries = [
        PreviewEntry::new("demo--card--a", "card", "A", empty),
        PreviewEntry::new("demo--card--a", "card", "a", empty),
    ];
    let message = duplicate_ids(&entries).unwrap();
    assert!(message.contains("(\"A\", \"a\")"), "{message}");
}

#[test]
fn a_canvas_for_an_entry_is_seeded_from_its_env() {
    let entry = PreviewEntry::new("demo--card--a", "card", "A", empty).locale("ar");
    let ctx = PreviewCtx::for_entry(&entry);
    assert_eq!(ctx.globals().locale().get().as_deref(), Some("ar"));
    assert_eq!(ctx.globals().mode().get(), None);
    assert_eq!(ctx.actions().count(), 0);
}

#[test]
fn a_host_shares_its_log_and_globals_with_every_canvas_it_builds() {
    let actions = crate::preview::ActionLog::new();
    let globals = crate::preview::Globals::new();
    let ctx = PreviewCtx::default()
        .with_actions(actions)
        .with_globals(globals);
    ctx.clone().actions().log("on_press", Vec::new());
    ctx.globals().mode().set(Some("dark".to_string()));
    assert_eq!(actions.count_of("on_press"), 1);
    assert_eq!(globals.mode().get().as_deref(), Some("dark"));
}

fn titled(id: &'static str, component: &'static str, title: &'static str) -> PreviewEntry {
    PreviewEntry::new(id, component, "A", empty).title(title)
}

#[test]
fn a_blank_request_names_nothing() {
    assert_eq!(
        PreviewRequest::new(Some(" "), Some("")),
        PreviewRequest::default()
    );
    assert_eq!(PreviewRequest::new(None, None), PreviewRequest::default());
}

#[test]
fn a_request_carries_the_id_and_the_component() {
    let request = PreviewRequest::new(Some("demo--card--a"), Some("card"));
    assert_eq!(request.id.as_deref(), Some("demo--card--a"));
    assert_eq!(request.component.as_deref(), Some("card"));
}

#[test]
fn a_component_matches_by_name_or_by_the_title_last_segment() {
    let request = PreviewRequest::new(None, Some("Button"));
    assert!(request.matches_component(&titled("a", "Button", "Inputs/Button")));
    assert!(request.matches_component(&titled("b", "primary", "Inputs/Button")));
    assert!(!request.matches_component(&titled("c", "field", "Inputs/Field")));
    assert!(PreviewRequest::default().matches_component(&titled("c", "field", "Inputs/Field")));
}

#[test]
fn an_id_resolves_before_a_component() {
    let entries = [
        titled("a", "button", "Inputs/Button"),
        titled("b", "field", "Inputs/Field"),
    ];
    let both = PreviewRequest::new(Some("b"), Some("button"));
    assert_eq!(both.resolve(&entries).map(|entry| entry.id), Some("b"));
    let component = PreviewRequest::new(Some("gone"), Some("field"));
    assert_eq!(component.resolve(&entries).map(|entry| entry.id), Some("b"));
    assert!(
        PreviewRequest::new(None, Some("gone"))
            .resolve(&entries)
            .is_none()
    );
}
