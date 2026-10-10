use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle, Role, box_item};

use super::sample;
use crate::button::{ButtonProps, button};
use crate::drawer::{DrawerProps, drawer};

#[derive(Clone, Copy, telar::PreviewArg)]
enum Side {
    Left,
    Right,
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(drawer: DrawerProps, "Open", |p| {
            let open = p.signal("open", true);
            let reopen = button(
                ButtonProps::props()
                    .label("Open the drawer")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
                Children::default(),
            )?;
            let side = match p.arg("side", Side::Left) {
                Side::Left => "left",
                Side::Right => "right",
            };
            let panel = drawer(
                DrawerProps::props()
                    .open(open)
                    .side(side)
                    .width(p.arg("width", 0.0f32))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                sample::children(|| {
                    Ok(vec![
                        sample::text("Filters")?,
                        sample::text("Tap outside the panel to close it.")?,
                    ])
                }),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(reopen), panel],
            )?)
        })
        .title("Overlays/Drawer")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(by_role(Role::Dialog).named("Drawer"))?;
            canvas.expect_text("Filters")?;
            canvas.click(by_text("Open the drawer"))?;
            canvas.expect_absent(by_role(Role::Dialog))?;
            canvas.expect_no_text("Filters")?;
            canvas.expect_action("on_close", 1)?;
            canvas.click(by_role(Role::Button).named("Open the drawer"))?;
            canvas.expect_text("Filters")
        }),
    ]
}
