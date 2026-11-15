use std::time::Duration;

use super::sample::Given;
use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Role, Side};

use crate::inputs::button::button::{ButtonProps, button};
use crate::overlays::tooltip::tooltip::{TooltipProps, tooltip};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tooltip: TooltipProps, "Default", |p| {
            let size = p.arg("size", ControlSize::Regular);
            tooltip(
                TooltipProps::props()
                    .text(p.arg("text", "Save"))
                    .shortcut(p.arg("shortcut", "Mod+S"))
                    .description(p.arg("description", "Writes the file to disk."))
                    .side(p.arg("side", Side::Below))
                    .size(size)
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .children(Children::one(move || {
                        button(ButtonProps::props().label("Hover me").size(size).build())
                    }))
                    .build(),
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
