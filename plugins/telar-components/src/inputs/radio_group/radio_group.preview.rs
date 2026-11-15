use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use super::sample::Given;
use telar::{Color, Key, NamedKey, Role};

use crate::inputs::radio_group::radio_group::{RadioGroupProps, radio_group};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(radio_group: RadioGroupProps, "Default", |p| {
            radio_group(
                RadioGroupProps::props()
                    .items(vec!["Small", "Medium", "Large"])
                    .label("Size")
                    .on_select(Rc::new(|_| {}))
                    .selected(p.signal("selected", Some(1usize)))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .build(),
            )
        })
        .title("Inputs/Radio group")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Radio).named("Large"))?;
            canvas.expect_toggled(by_role(Role::Radio).named("Large"), true)?;
            canvas.expect_toggled(by_role(Role::Radio).named("Medium"), false)?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.expect_toggled(by_role(Role::Radio).named("Small"), true)?;
            canvas.expect_focused(by_role(Role::Radio).named("Small"))
        }),
    ]
}
