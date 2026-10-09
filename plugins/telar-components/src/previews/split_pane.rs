use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use super::sample;
use crate::split_pane::{SizedPane, SplitDirection, SplitPaneProps, split_pane};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(split_pane: SplitPaneProps, "Default", |p| {
            split_pane(
                SplitPaneProps::props()
                    .direction(p.arg("direction", SplitDirection::Row))
                    .sized(p.arg("sized", SizedPane::First))
                    .size(p.signal("size", 240.0f32))
                    .collapsed(p.signal("collapsed", false))
                    .min(p.arg("min", 120.0f32))
                    .max(p.arg("max", 0.0f32))
                    .collapsible(p.arg("collapsible", true))
                    .build(),
                sample::children(|| {
                    Ok(vec![
                        sample::text("Sidebar")?,
                        sample::text("Editor")?,
                    ])
                }),
            )
        })
        .title("Workbench/Split pane")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
