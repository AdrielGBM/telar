use telar::Role;
use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};

use crate::title_bar::{WindowControlsProps, window_controls};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(window_controls: WindowControlsProps, "Default", |p| {
            window_controls(
                WindowControlsProps::props()
                    .minimize(p.arg("minimize", true))
                    .maximize(p.arg("maximize", true))
                    .build(),
            )
        })
        .title("Chrome/Window controls")
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
