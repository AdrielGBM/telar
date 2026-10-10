use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Key, NamedKey, Role};

use crate::text_field::{TextFieldProps, text_field};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(text_field: TextFieldProps, "Default", |p| {
            text_field(
                TextFieldProps::props()
                    .label(p.arg("label", "Name"))
                    .placeholder(p.arg("placeholder", "Type here"))
                    .width(p.arg("width", 300.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Text field")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect_text("Type here")?;
            canvas.type_text(by_role(Role::TextInput).named("Name"), "Ada")?;
            canvas.expect_text("Ada")?;
            canvas.expect_no_text("Type here")?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_submit", 1)
        }),
        preview!(text_field: TextFieldProps, "Bound value", |p| {
            let value = p.signal("value", String::new());
            text_field(
                TextFieldProps::props()
                    .label("Name")
                    .placeholder("Type here")
                    .value(value)
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Text field")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.type_text(by_role(Role::TextInput), "Ada")?;
            canvas.expect_text("Ada")?;
            canvas.press(Key::Named(NamedKey::Backspace))?;
            canvas.expect_no_text("Ada")?;
            canvas.expect_text("Ad")
        }),
    ]
}
