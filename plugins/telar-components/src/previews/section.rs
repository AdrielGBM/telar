use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use super::sample;
use crate::section::{SectionProps, section};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(section: SectionProps, "Default", |p| {
            section(
                SectionProps::props()
                    .title(p.arg("title", "Profile"))
                    .build(),
                sample::children(|| {
                    Ok(vec![
                        sample::text("Your name and photo are shown to other members.")?,
                        sample::text("Only you can see your email address.")?,
                    ])
                }),
            )
        })
        .title("Layout/Section")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes")),
    ]
}
