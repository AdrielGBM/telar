use std::time::Duration;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Role};

use super::sample;
use crate::button::{ButtonProps, button};
use crate::tooltip::{TooltipProps, tooltip};

#[derive(Clone, Copy, telar::PreviewArg)]
enum Side {
    Bottom,
    Top,
    Start,
    End,
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tooltip: TooltipProps, "Default", |p| {
            let side = match p.arg("side", Side::Bottom) {
                Side::Bottom => "bottom",
                Side::Top => "top",
                Side::Start => "start",
                Side::End => "end",
            };
            tooltip(
                TooltipProps::props()
                    .text(p.arg("text", "Save"))
                    .shortcut(p.arg("shortcut", "Ctrl+S"))
                    .description(p.arg("description", "Writes the file to disk."))
                    .side(side)
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                sample::children(|| {
                    Ok(vec![button(
                        ButtonProps::props().label("Hover me").build(),
                        Children::default(),
                    )?])
                }),
            )
        })
        .title("Overlays/Tooltip")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect_no_text("Writes the file to disk.")?;
            canvas.hover(by_role(Role::Button).named("Hover me"))?;
            canvas.advance(Duration::from_secs(1))?;
            canvas.expect_text("Writes the file to disk.")
        }),
    ]
}
