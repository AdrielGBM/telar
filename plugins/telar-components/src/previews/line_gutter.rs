use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{LayoutStyle, TextStyle};

use crate::line_gutter::LineGutter;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(line_gutter, "Default", |p| {
            let first = p.arg("first_line", 1u32) as usize;
            let count = p.arg("line_count", 12u32) as usize;
            LineGutter::starting_at(
                move || first,
                move || count,
                LayoutStyle::new(),
                || TextStyle::new(13.0, telar::use_theme_tokens().muted()),
            )
        })
        .title("Workbench/Line gutter")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes")),
    ]
}
