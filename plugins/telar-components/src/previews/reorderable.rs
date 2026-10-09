use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Reactive, signal};

use super::sample;
use crate::reorderable::{ReorderableProps, reorderable};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(reorderable: ReorderableProps, "Strip", |p| {
            let items = signal(vec!["Inbox", "Drafts", "Sent", "Archive"]);
            reorderable(
                ReorderableProps::props()
                    .count(Reactive::of(move || items.get().len()))
                    .item(Rc::new(move |index| {
                        sample::tile(Reactive::of(move || items.get()[index].to_string()))
                    }))
                    .on_move(Rc::new(move |from, to| {
                        items.update(|items| {
                            telar::apply_move(items, from, to);
                        });
                    }))
                    .row(p.arg("row", true))
                    .gap(p.arg("gap", 8.0f32))
                    .drag_threshold(p.arg("drag_threshold", 4.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Layout/Reorderable")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
