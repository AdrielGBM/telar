use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Container, LayoutError, LayoutItem, LayoutStyle, Reactive, Text, box_item};

use crate::window_frame::{SurfaceFrameStyle, WindowControls, window_frame};

fn line(content: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || content.to_string(),
        LayoutStyle::new(),
        move |t| t.with_color(telar::use_theme_tokens().ink()),
    )?;
    Ok(box_item(text))
}

fn token(read: fn(&dyn telar::ThemeTokens) -> telar::Color) -> Reactive<telar::Color> {
    Reactive::of(move || read(&*telar::use_theme_tokens()))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(window_frame, "Default", |p| {
            let style = SurfaceFrameStyle {
                radius: telar::use_theme_tokens().radius(),
                font_size: 13.0,
                controls: WindowControls {
                    drag: false,
                    minimize: p.arg("minimize", true),
                    maximize: p.arg("maximize", true),
                },
                body_inset: p.arg("body_inset", 12.0f32),
                ..SurfaceFrameStyle::default()
            }
            .background(token(|t| t.surface()))
            .title_bar(token(|t| t.surface_alt()))
            .title_text(token(|t| t.ink()))
            .close(token(|t| t.ink()))
            .control_hover(token(|t| t.highlight_med()))
            .close_hover(token(|t| t.error()));
            let body = Container::new(
                LayoutStyle::new()
                    .flex_column()
                    .gap(8.0)
                    .width(320.0)
                    .height(160.0),
                vec![
                    line("Telar can draw its own window chrome.")?,
                    line("Drag the corner grip to resize.")?,
                ],
            )?;
            window_frame(
                p.arg("title", "Settings"),
                None,
                style,
                Rc::new(|| {}),
                Box::new(body),
                Some(Rc::new(|_, _| {})),
            )
        })
        .title("Chrome/Window frame")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
