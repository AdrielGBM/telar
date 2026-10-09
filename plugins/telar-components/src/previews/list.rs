use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutStyle};

use crate::list::{GroupProps, ItemProps, SeparatorProps, group, item, separator};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(item: ItemProps, "Default", |p| {
            item(
                ItemProps::props()
                    .label(p.arg("label", "Rename"))
                    .disabled(p.arg("disabled", false))
                    .checked(p.arg("checked", false))
                    .hint(p.arg("hint", "F2"))
                    .build(),
                Children::default(),
            )
        })
        .title("Layout/List")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes")),
        preview!(group: GroupProps, "Grouped rows", |p| {
            let row = |label: &'static str, hint: &'static str| {
                crate::list::item(
                    ItemProps::props().label(label).hint(hint).build(),
                    Children::default(),
                )
            };
            let rows = vec![
                group(
                    GroupProps::props().label(p.arg("label", "Edit")).build(),
                    Children::default(),
                )?,
                row("Undo", "Ctrl+Z")?,
                row("Redo", "Ctrl+Y")?,
                separator(SeparatorProps::props().build(), Children::default())?,
                group(GroupProps::props().label("View").build(), Children::default())?,
                row("Zoom in", "Ctrl+=")?,
                row("Zoom out", "Ctrl+-")?,
            ];
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column().width(240.0),
                rows,
            )?)
        })
        .title("Layout/List")
        .layout(Layout::Padded),
    ]
}
