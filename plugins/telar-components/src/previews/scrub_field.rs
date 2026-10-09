use telar::Children;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::scrub_field::{ScrubFieldProps, scrub_field};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(scrub_field: ScrubFieldProps, "Default", |p| {
            scrub_field(
                ScrubFieldProps::props()
                    .value(p.signal("value", 12.0f32))
                    .label(p.arg("label", "Width"))
                    .min(p.arg("min", 0.0f32))
                    .max(p.arg("max", 100.0f32))
                    .step(p.arg("step", 1.0f32))
                    .pixels_per_step(p.arg("pixels_per_step", 4.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Scrub field")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
