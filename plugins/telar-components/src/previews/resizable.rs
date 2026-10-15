use telar::preview::{Matrix, PreviewEntry, by_role, preview};
use telar::{
    Border, Key, LayoutStyle, NamedKey, RectStyle, Role, ShapeStyle, Size, StyledContainer,
    box_item, signal,
};

use super::sample;
use crate::resizable::{ResizableProps, ResizeEdges, ResizeGrowth, resizable};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(resizable: ResizableProps, "Default", |p| {
            let size = signal(Size::new(
                p.arg("width", 240.0f32),
                p.arg("height", 160.0f32),
            ));
            resizable(
                ResizableProps::props()
                    .size(size)
                    .min(Size::new(120.0, 80.0))
                    .max(Size::new(
                        p.arg("max_width", 360.0f32),
                        p.arg("max_height", 0.0f32),
                    ))
                    .edges(p.arg("edges", ResizeEdges::All))
                    .growth(p.arg("growth", ResizeGrowth::Anchored))
                    .step(p.arg("step", 16.0f32))
                    .build(),
                sample::children(|| {
                    let panel = StyledContainer::new(
                        LayoutStyle::column().flex_grow(1.0).padding_all(12.0),
                        |_| {
                            let tokens = telar::use_theme_tokens();
                            RectStyle::default()
                                .with_fill(tokens.surface_alt())
                                .with_border(Border::uniform(tokens.border(), 1.0))
                        },
                        vec![sample::text("Panel")?],
                    )?;
                    Ok(vec![box_item(panel)])
                }),
            )
        })
        .title("Workbench/Resizable")
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let width = by_role(Role::Splitter).named("Resize width");
            let height = by_role(Role::Splitter).named("Resize height");
            canvas.tab()?;
            canvas.expect_focused(width.clone())?;
            canvas.press(Key::Named(NamedKey::ArrowLeft))?;
            canvas.expect_value(width.clone(), 224.0)?;
            canvas.press(Key::Named(NamedKey::End))?;
            canvas.expect_value(width.clone(), 360.0)?;
            canvas.press(Key::Named(NamedKey::Home))?;
            canvas.expect_value(width.clone(), 120.0)?;
            canvas.tab()?;
            canvas.expect_focused(height.clone())?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.expect_value(height, 176.0)?;
            canvas.drag(width.clone(), 40.0, 0.0)?;
            canvas.expect_value(width, 160.0)
        }),
    ]
}
