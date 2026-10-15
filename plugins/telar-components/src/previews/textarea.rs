use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Key, NamedKey, Role};

use crate::textarea::{TextareaProps, textarea};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(textarea: TextareaProps, "Default", |p| {
            textarea(
                TextareaProps::props()
                    .label(p.arg("label", "Notes"))
                    .placeholder(p.arg("placeholder", "Anything we should know?"))
                    .width(p.arg("width", 300.0f32))
                    .rows(3)
                    .max_rows(6)
                    .counter(p.arg("counter", true))
                    .max_length(120)
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Textarea")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect_text("Anything we should know?")?;
            canvas.expect_text("0 / 120")?;
            canvas.type_text(
                by_role(Role::MultilineTextInput).named("Notes"),
                "Ring twice",
            )?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.type_text(by_role(Role::MultilineTextInput), "Leave it by the door")?;
            canvas.expect_text("31 / 120")?;
            canvas.expect_no_text("Anything we should know?")
        }),
    ]
}
