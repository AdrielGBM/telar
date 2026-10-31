use telar::Color;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::spinner::{SpinnerProps, spinner};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(spinner: SpinnerProps, "Default", |p| {
            spinner(
                SpinnerProps::props()
                    .color(p.arg("color", Color::TRANSPARENT))
                    .size(p.arg("size", 0.0f32))
                    .build(),
            )
        })
        .title("Feedback/Spinner")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
