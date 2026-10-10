use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle, Reactive, Role};

use crate::checkbox::{CheckboxProps, checkbox};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(checkbox: CheckboxProps, "Default", |p| {
            checkbox(
                CheckboxProps::props()
                    .checked(p.signal("checked", false))
                    .label(p.arg("label", "I agree to the terms"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Checkbox")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            let agree = by_role(Role::CheckBox).named("I agree to the terms");
            canvas.click(agree.clone())?;
            canvas.expect_toggled(agree.clone(), true)?;
            canvas.click(agree.clone())?;
            canvas.expect_toggled(agree, false)?;
            canvas.expect_action("on_toggle", 2)
        }),
        preview!(checkbox: CheckboxProps, "Bound", |p| {
            let agree = p.signal("agree", false);
            let check = checkbox(
                CheckboxProps::props()
                    .checked(agree)
                    .label("I agree")
                    .build(),
                Children::default(),
            )?;
            let echo = super::sample::text_of(Reactive::of(move || format!("agree · {}", agree.get())))?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column().gap(8.0),
                vec![check, echo],
            )?)
        })
        .title("Inputs/Checkbox")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_text("agree · false")?;
            canvas.click(by_role(Role::CheckBox).named("I agree"))?;
            canvas.expect_text("agree · true")
        }),
    ]
}
