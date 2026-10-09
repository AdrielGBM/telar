use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::progress::{ProgressProps, progress};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(progress: ProgressProps, "Default", |p| {
            progress(
                ProgressProps::props()
                    .value(p.arg("value", 0.6f32))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .track_color(p.arg("track_color", Color::TRANSPARENT))
                    .width(p.arg("width", 0.0f32))
                    .height(p.arg("height", 0.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Feedback/Progress")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
