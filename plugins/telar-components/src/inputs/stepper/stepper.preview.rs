use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use super::sample::Given;
use telar::{Accessible, Color, Key, NamedKey, Role};

use crate::inputs::stepper::stepper::{StepperProps, stepper};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(stepper: StepperProps, "Default", |p| {
            stepper(
                StepperProps::props()
                    .on_change(Rc::new(|_| {}))
                    .value(p.signal("value", 2.0f32))
                    .range(p.arg("min", 0.0f32)..=p.arg("max", 10.0f32))
                    .step(p.arg("step", 1.0f32))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .build(),
            )
            .map(|stepper| stepper.a11y_label(|| "Quantity"))
        })
        .title("Inputs/Stepper")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_value(by_role(Role::SpinButton).named("Quantity"), 2.0)?;
            canvas.tab()?;
            canvas.press(Key::Named(NamedKey::ArrowUp))?;
            canvas.expect_value(by_role(Role::SpinButton), 3.0)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.expect_value(by_role(Role::SpinButton), 1.0)?;
            canvas.expect_action("on_change", 3)
        }),
    ]
}
