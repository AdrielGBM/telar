use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color, Reactive};

use crate::chip::{ChipProps, chip};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(chip: ChipProps, "Default", |p| {
            chip(
                ChipProps::props()
                    .label(p.arg("label", "Design"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )
        })
        .title("Display/Chip")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(chip: ChipProps, "Removable", |p| {
            chip(
                ChipProps::props()
                    .label(p.arg("label", "Rust"))
                    .color(Reactive::of(|| telar::use_theme_tokens().success()))
                    .on_close(Rc::new(|| {}))
                    .build(),
                Children::default(),
            )
        })
        .title("Display/Chip")
        .layout(Layout::Centered),
    ]
}
