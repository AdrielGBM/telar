use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Key, NamedKey, Role};

use crate::slider::{SliderProps, slider};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(slider: SliderProps, "Default", |p| {
            slider(
                SliderProps::props()
                    .value(p.signal("value", 0.4f32))
                    .label(p.arg("label", "Volume"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .track_color(p.arg("track_color", Color::TRANSPARENT))
                    .width(p.arg("width", 0.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Slider")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::Slider).named("Volume"))?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_value(by_role(Role::Slider), 0.45)?;
            canvas.expect_action("on_change", 1)
        }),
        preview!(slider: SliderProps, "Stepped range", |p| {
            slider(
                SliderProps::props()
                    .value(p.signal("value", 40.0f32))
                    .label("Brightness")
                    .min(p.arg("min", 0.0f32))
                    .max(p.arg("max", 100.0f32))
                    .step(p.arg("step", 10.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Slider")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            let brightness = by_role(Role::Slider).named("Brightness");
            canvas.tab()?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_value(brightness.clone(), 50.0)?;
            canvas.press(Key::Named(NamedKey::ArrowLeft))?;
            canvas.press(Key::Named(NamedKey::ArrowLeft))?;
            canvas.expect_value(brightness, 30.0)
        }),
    ]
}
