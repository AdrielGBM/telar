use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::toggle::{ToggleProps, toggle};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(toggle: ToggleProps, "Default", |p| {
            toggle(
                ToggleProps::props()
                    .checked(p.signal("checked", true))
                    .label(p.arg("label", "Notifications"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Toggle")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
