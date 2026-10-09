use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use super::sample;
use crate::list::{ItemProps, SeparatorProps, item, separator};
use crate::menu::{MenuProps, menu};

fn actions() -> Children {
    sample::children(|| {
        let row = |label: &'static str, hint: &'static str| {
            item(
                ItemProps::props().label(label).hint(hint).build(),
                Children::default(),
            )
        };
        Ok(vec![
            row("New file", "Ctrl+N")?,
            row("Open…", "Ctrl+O")?,
            separator(SeparatorProps::props().build(), Children::default())?,
            item(
                ItemProps::props().label("Close").disabled(true).build(),
                Children::default(),
            )?,
        ])
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(menu: MenuProps, "Default", |p| {
            menu(
                MenuProps::props()
                    .label(p.arg("label", "File"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .bordered(p.arg("bordered", false))
                    .caret(p.arg("caret", true))
                    .stretch(p.arg("stretch", false))
                    .build(),
                actions(),
            )
        })
        .title("Overlays/Menu")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
