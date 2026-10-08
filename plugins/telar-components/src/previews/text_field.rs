use telar::Children;
use telar::preview::{Layout, PreviewEntry, preview};

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
        .layout(Layout::Centered),
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
        .tags(&["stateful"]),
    ]
}
