use telar::preview::{Layout, Matrix, PreviewEntry, by_text, preview};
use telar::{Container, LayoutError, LayoutStyle};

use crate::list::{GroupProps, ItemProps, SeparatorProps, group, item, separator};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(item: ItemProps, "Default", |p| {
            item(
                ItemProps::props()
                    .label(p.arg("label", "Rename"))
                    .disabled(p.arg("disabled", false))
                    .checked(p.arg("checked", false))
                    .shortcut(p.arg("shortcut", "F2"))
                    .build(),
            )
        })
        .title("Layout/List")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_text("Rename"))?;
            canvas.expect_action("on_press", 1)
        }),
        preview!(group: GroupProps, "Grouped rows", |p| {
            let row = |label: &'static str, shortcut: &'static str| {
                crate::list::item(ItemProps::props().label(label).shortcut(shortcut).build())
            };
            let rows = vec![
                group(GroupProps::props().label(p.arg("label", "Edit")).build())?,
                row("Undo", "Mod+Z")?,
                row("Redo", "Mod+Shift+Z")?,
                separator(SeparatorProps::props().build())?,
                group(GroupProps::props().label("View").build())?,
                row("Zoom in", "Mod+=")?,
                row("Zoom out", "Mod+-")?,
            ];
            Ok::<_, LayoutError>(Container::new(LayoutStyle::column().width(240.0), rows)?)
        })
        .title("Layout/List")
        .layout(Layout::Padded),
    ]
}
