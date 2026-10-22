use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutStyle};

use crate::heading::{HeadingProps, heading};
use crate::metrics;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(heading: HeadingProps, "Default", |p| {
            heading(
                HeadingProps::props()
                    .text(p.arg("text", "Heading"))
                    .level(p.arg("level", 2u8))
                    .build(),
                Children::default(),
            )
        })
        .title("Layout/Heading")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(heading: HeadingProps, "Levels", |_p| {
            let titles = (1..=6u8)
                .map(|level| {
                    heading(
                        HeadingProps::props()
                            .text(format!("Heading {level}"))
                            .level(level)
                            .build(),
                        Children::default(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let column = || LayoutStyle::column().gap(metrics::gap_md());
            Ok::<_, LayoutError>(Container::new(column(), titles)?.styled_by(column))
        })
        .title("Layout/Heading")
        .layout(Layout::Centered),
    ]
}
