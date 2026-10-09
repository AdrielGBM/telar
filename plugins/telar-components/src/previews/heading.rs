use telar::Children;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::heading::{HeadingProps, heading};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(heading: HeadingProps, "Default", |p| {
            heading(
                HeadingProps::props()
                    .text(p.arg("text", "Account settings"))
                    .build(),
                Children::default(),
            )
        })
        .title("Layout/Heading")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
