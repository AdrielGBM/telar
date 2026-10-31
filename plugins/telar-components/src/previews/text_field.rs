use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Key, NamedKey, Role};

use crate::text_field::{TextFieldProps, text_field};

const WRAPPED: &str = "Ring twice and leave the parcel with the neighbour at number twelve if nobody answers; the side gate is open until six.";

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(text_field: TextFieldProps, "Default", |p| {
            text_field(
                TextFieldProps::props()
                    .label(p.arg("label", "Name"))
                    .placeholder(p.arg("placeholder", "Type here"))
                    .width(p.arg("width", 300.0f32))
                    .build(),
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
        preview!(text_field: TextFieldProps, "Multiline", |p| {
            text_field(
                TextFieldProps::props()
                    .label(p.arg("label", "Notes"))
                    .placeholder(p.arg("placeholder", "Anything we should know?"))
                    .width(p.arg("width", 300.0f32))
                    .multiline(true)
                    .rows(3)
                    .max_rows(6)
                    .counter(p.arg("counter", true))
                    .max_length(120)
                    .build(),
            )
        })
        .title("Inputs/Text field")
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
        preview!(text_field: TextFieldProps, "Multiline wrapped", |p| {
            let value = p.signal("value", WRAPPED.to_string());
            text_field(
                TextFieldProps::props()
                    .label("Delivery notes")
                    .value(value)
                    .width(p.arg("width", 300.0f32))
                    .multiline(true)
                    .rows(2)
                    .max_rows(6)
                    .build(),
            )
        })
        .title("Inputs/Text field")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect_text(WRAPPED)?;
            canvas.type_text(
                by_role(Role::MultilineTextInput).named("Delivery notes"),
                "",
            )?;
            for _ in 0..6 {
                canvas.press(Key::Named(NamedKey::ArrowDown))?;
            }
            canvas.press(Key::Char('!'))?;
            canvas.expect_text(&format!("{WRAPPED}!"))
        }),
    ]
}
