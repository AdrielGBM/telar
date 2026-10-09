use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle};

use crate::radio::{RadioProps, radio};

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(radio: RadioProps, "Group", |p| {
            let selected = p.signal("selected", 1u32);
            let color = p.arg("color", Color::TRANSPARENT);
            let options = (0u32..)
                .zip(SIZES)
                .map(|(value, label)| {
                    radio(
                        RadioProps::props()
                            .selected(selected)
                            .value(value)
                            .label(label)
                            .color(color)
                            .build(),
                        Children::default(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column().gap(8.0),
                options,
            )?)
        })
        .title("Inputs/Radio")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
