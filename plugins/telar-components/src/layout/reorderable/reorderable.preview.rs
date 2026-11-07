use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Key, NamedKey, Reactive, Role, signal};

use super::sample;
use crate::layout::reorder_zones::reorder_zones::DropPreview;
use crate::layout::reorderable::reorderable::{ReorderableProps, reorderable};

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
                    .drop_preview(p.arg("drop_preview", DropPreview::Gap))
                    .build(),
            )
        })
        .title("Layout/Reorderable")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let items = by_role(Role::ListItem);
            canvas.drag(items.clone().named("Inbox"), 90.0, 0.0)?;
            sample::expect_names(canvas, &items, &["Drafts", "Inbox", "Sent", "Archive"])?;
            canvas.expect_action("on_move", 1)?;
            canvas.click(items.clone().named("Archive"))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.press(Key::Named(NamedKey::ArrowLeft))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            sample::expect_names(canvas, &items, &["Drafts", "Inbox", "Archive", "Sent"])?;
            canvas.expect_action("on_move", 2)
        }),
    ]
}
