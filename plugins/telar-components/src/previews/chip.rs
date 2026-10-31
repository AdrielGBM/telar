use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Color, Reactive, Role};

use crate::chip::{ChipProps, chip};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(chip: ChipProps, "Default", |p| {
            chip(
                ChipProps::props()
                    .label(p.arg("label", "Design"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
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
            )
        })
        .title("Display/Chip")
        .layout(Layout::Centered)
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Remove Rust"))?;
            canvas.expect_action("on_close", 1)
        }),
    ]
}
