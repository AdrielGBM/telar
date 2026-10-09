use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

use crate::color_picker::{ColorPickerProps, color_picker};

fn palette() -> Vec<Color> {
    let tokens = telar::use_theme_tokens();
    vec![
        tokens.primary(),
        tokens.info(),
        tokens.success(),
        tokens.warning(),
        tokens.error(),
        tokens.muted(),
    ]
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(color_picker: ColorPickerProps, "Default", |p| {
            let value = p.signal("value", Color::rgba(0.15, 0.39, 0.92, 1.0));
            color_picker(
                ColorPickerProps::props()
                    .value(value)
                    .alpha(p.arg("alpha", true))
                    .label(p.arg("label", "Accent"))
                    .width(p.arg("width", 0.0f32))
                    .swatches(palette())
                    .swatch_names(
                        ["Primary", "Info", "Success", "Warning", "Error", "Muted"]
                            .map(String::from)
                            .to_vec(),
                    )
                    .build(),
                Children::default(),
            )
        })
        .title("Workbench/Color picker")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
