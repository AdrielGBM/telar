use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{
    BorderRadius, Children, LayoutError, LayoutStyle, RectStyle, ShapeStyle, StyledContainer,
    box_item,
};

use crate::handle::{HandleProps, handle};

const TRACK: f32 = 240.0;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(handle: HandleProps, "On a track", |p| {
            let dot = handle(
                HandleProps::props()
                    .value(p.signal("value", 40.0f32))
                    .to_value(Rc::new(|x, _| x / TRACK * 100.0))
                    .to_point(Rc::new(|value| (value / 100.0 * TRACK, 12.0)))
                    .min(p.arg("min", 0.0f32))
                    .max(p.arg("max", 100.0f32))
                    .step(p.arg("step", 1.0f32))
                    .size(p.arg("size", 12.0f32))
                    .label(p.arg("label", "Opacity"))
                    .build(),
                Children::default(),
            )?;
            let track = StyledContainer::new(
                LayoutStyle::new().width(TRACK).height(24.0),
                |_| {
                    RectStyle::default()
                        .with_fill(telar::use_theme_tokens().surface_alt())
                        .with_radius(BorderRadius::all(12.0))
                },
                vec![dot],
            )?;
            Ok::<_, LayoutError>(box_item(track))
        })
        .title("Inputs/Handle")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
