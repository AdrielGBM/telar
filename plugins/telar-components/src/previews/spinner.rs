use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::spinner::{SpinnerProps, spinner};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(spinner: SpinnerProps, "Default", |p| {
            spinner(
                SpinnerProps::props()
                    .color(p.arg("color", Color::TRANSPARENT))
                    .size(p.arg("size", 0.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Feedback/Spinner")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
