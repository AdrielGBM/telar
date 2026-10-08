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
