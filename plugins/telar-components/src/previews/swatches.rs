use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::swatches::{SwatchesProps, swatches};

fn palette() -> Vec<Color> {
    let tokens = telar::use_theme_tokens();
    vec![
        tokens.primary(),
        tokens.info(),
        tokens.success(),
        tokens.warning(),
        tokens.error(),
    ]
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(swatches: SwatchesProps, "Theme palette", |p| {
            swatches(
                SwatchesProps::props()
                    .colors(palette())
                    .names(
                        ["Primary", "Info", "Success", "Warning", "Error"]
                            .map(String::from)
                            .to_vec(),
                    )
                    .selected(p.signal("selected", Some(0u32)))
                    .size(p.arg("size", 20.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Swatches")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
