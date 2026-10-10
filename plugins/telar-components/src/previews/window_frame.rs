use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Container, LayoutError, LayoutItem, LayoutStyle, Reactive, Role, Text, box_item};

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
            .close_hover(token(|t| t.error()))
            .close_hover_glyph(token(|t| t.on_error()));
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
            let actions = p.actions();
            window_frame(
                p.arg("title", "Settings"),
                None,
                style,
                Rc::new(move || actions.log("on_close", Vec::new())),
                Box::new(body),
                Some(Rc::new(|_, _| {})),
            )
        })
        .title("Chrome/Window frame")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::Button).named("Minimize"))?;
            canvas.expect(by_role(Role::Button).named("Maximize"))?;
            canvas.click(by_role(Role::Button).named("Close"))?;
            canvas.expect_action("on_close", 1)
        }),
    ]
}
