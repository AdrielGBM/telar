use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Reactive, Role};

use crate::button::{ButtonProps, ButtonVariant, button};
use crate::metrics;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(button: ButtonProps, "Default", |p| {
            button(
                ButtonProps::props()
                    .label(p.arg("label", "Save"))
                    .variant(p.arg("variant", ButtonVariant::Solid))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
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
                ("Solid", ButtonVariant::Solid),
                ("Outline", ButtonVariant::Outline),
                ("Soft", ButtonVariant::Soft),
                ("Ghost", ButtonVariant::Ghost),
            ]
            .into_iter()
            .map(|(label, variant)| {
                button(
                    ButtonProps::props()
                        .label(label)
                        .variant(variant)
                        .color(color)
                        .build(),
                    Children::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
            let row = Container::new(LayoutStyle::row().gap(metrics::gap_md()), buttons)?
                .styled_by(|| LayoutStyle::row().gap(metrics::gap_md()));
            Ok::<_, LayoutError>(row)
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(button: ButtonProps, "Counting presses", |p| {
            let presses = p.signal("presses", 0u32);
            button(
                ButtonProps::props()
                    .label(Reactive::of(move || format!("Pressed {} times", presses.get())))
                    .on_press(Rc::new(move || presses.update(|count| *count += 1)))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Pressed 0 times"))?;
            canvas.expect_text("Pressed 1 times")?;
            canvas.expect_action("on_press", 1)?;
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::Button))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_text("Pressed 2 times")
        }),
    ]
}
