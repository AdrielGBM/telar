use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Key, NamedKey, Role};

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
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.tab()?;
            canvas.press(Key::Named(NamedKey::ArrowUp))?;
            canvas.expect_value(by_role(Role::SpinButton).named("Width 13"), 13.0)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.expect_value(by_role(Role::SpinButton), 11.0)
        }),
    ]
}
