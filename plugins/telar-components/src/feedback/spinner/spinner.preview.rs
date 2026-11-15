use telar::{Accessible, Color};
use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use super::sample::Given;

use crate::feedback::spinner::spinner::{SpinnerProps, spinner};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(spinner: SpinnerProps, "Default", |p| {
            spinner(
                SpinnerProps::props()
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("diameter", None::<f32>), |props, value| props.diameter(value))
                    .build(),
            )
            .map(|ring| ring.a11y_label(|| "Loading"))
        })
        .title("Feedback/Spinner")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
