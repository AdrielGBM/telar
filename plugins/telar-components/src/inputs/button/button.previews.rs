use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Reactive, Role};

use crate::inputs::button::button::{ButtonProps, button};
use crate::metrics;
use telar::{ControlSize, Variant};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(button: ButtonProps, "Default", |p| {
            button(
                ButtonProps::props()
                    .label(p.arg("label", "Save"))
                    .size(p.arg("size", ControlSize::Regular))
                    .variant(p.arg("variant", Variant::Solid))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
            )
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Save"))?;
            canvas.expect_action("on_press", 1)
        }),
        preview!(button: ButtonProps, "Variants", |p| {
            let color = p.arg("color", Color::TRANSPARENT);
            let buttons = [
                ("Solid", Variant::Solid),
                ("Outline", Variant::Outline),
                ("Soft", Variant::Soft),
                ("Ghost", Variant::Ghost),
            ]
            .into_iter()
            .map(|(label, variant)| {
                button(
                    ButtonProps::props()
                        .label(label)
                        .variant(variant)
                        .color(color)
                        .build(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
            let row = Container::new(LayoutStyle::row().gap(metrics::gap_md(ControlSize::Regular)), buttons)?
                .styled_by(|| LayoutStyle::row().gap(metrics::gap_md(ControlSize::Regular)));
            Ok::<_, LayoutError>(row)
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(button: ButtonProps, "Counting presses", |p| {
            let presses = p.signal("presses", 0u32);
            button(
                ButtonProps::props()
                    .label(Reactive::of(move || {
                        format!("Pressed {} times", presses.get())
                    }))
                    .on_press(Rc::new(move || presses.update(|count| *count += 1)))
                    .build(),
            )
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Pressed 0 times"))?;
            canvas.expect_text("Pressed 1 times")?;
            canvas.expect_action("on_press", 1)?;
            canvas.click_outside(by_role(Role::Button))?;
            canvas.hover(by_role(Role::Button))?;
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::Button))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_text("Pressed 2 times")
        }),
    ]
}
