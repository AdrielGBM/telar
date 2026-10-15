use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Key, NamedKey, Role};

use crate::search_field::{SearchFieldProps, search_field};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(search_field: SearchFieldProps, "Default", |p| {
            let value = p.signal("query", String::new());
            search_field(
                SearchFieldProps::props()
                    .value(value)
                    .width(p.arg("width", 280.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Search field")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_absent(by_role(Role::Button).named("Clear search"))?;
            canvas.type_text(by_role(Role::TextInput).named("Search"), "tree")?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_submit", 1)?;
            canvas.click(by_role(Role::Button).named("Clear search"))?;
            canvas.expect_action("on_clear", 1)?;
            canvas.expect_absent(by_role(Role::Button).named("Clear search"))?;
            canvas.type_text(by_role(Role::TextInput), "list")?;
            canvas.press(Key::Named(NamedKey::Escape))?;
            canvas.expect_no_text("list")?;
            canvas.expect_action("on_clear", 2)
        }),
    ]
}
