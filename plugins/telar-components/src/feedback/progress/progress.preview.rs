use telar::{Accessible, Color};
use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use super::sample::Given;

use crate::feedback::progress::progress::{ProgressProps, progress};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(progress: ProgressProps, "Default", |p| {
            progress(
                ProgressProps::props()
                    .value(p.arg("value", 0.6f32))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .given(p.arg("track_color", None::<Color>), |props, color| props.track_color(color))
                    .given(p.arg("width", None::<f32>), |props, value| props.width(value))
                    .given(p.arg("height", None::<f32>), |props, value| props.height(value))
                    .build(),
            )
            .map(|bar| bar.a11y_label(|| "Uploading"))
        })
        .title("Feedback/Progress")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
