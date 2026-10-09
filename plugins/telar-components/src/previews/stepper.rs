use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::stepper::{StepperProps, stepper};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(stepper: StepperProps, "Default", |p| {
            stepper(
                StepperProps::props()
                    .value(p.signal("value", 2.0f32))
                    .min(p.arg("min", 0.0f32))
                    .max(p.arg("max", 10.0f32))
                    .step(p.arg("step", 1.0f32))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Stepper")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
