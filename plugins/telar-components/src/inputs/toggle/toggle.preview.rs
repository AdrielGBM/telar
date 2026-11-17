use super::sample::Given;
use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Color, Role};

use crate::inputs::toggle::toggle::{ToggleProps, toggle};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(toggle: ToggleProps, "Default", |p| {
            toggle(
                ToggleProps::props()
                    .on_toggle(|_| {})
                    .checked(p.signal("checked", true))
                    .label(p.arg("label", "Notifications"))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .build(),
            )
        })
        .title("Inputs/Toggle")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let notifications = by_role(Role::Switch).named("Notifications");
            canvas.expect_toggled(notifications.clone(), true)?;
            canvas.click(notifications.clone())?;
            canvas.expect_toggled(notifications, false)?;
            canvas.expect_action("on_toggle", 1)
        }),
    ]
}
