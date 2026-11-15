use std::rc::Rc;

use super::sample::Given;
use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, LayoutError, Role, Side, box_item};

use super::sample;
use crate::inputs::button::button::{ButtonProps, button};
use crate::overlays::drawer::drawer::{DrawerProps, drawer};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(drawer: DrawerProps, "Open", |p| {
            let open = p.signal("open", true);
            let reopen = button(
                ButtonProps::props()
                    .label("Open the drawer")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
            )?;
            let panel = drawer(
                DrawerProps::props()
                    .on_close(Rc::new(|| {}))
                    .control(open)
                    .side(p.arg("side", Side::Start))
                    .given(p.arg("extent", None::<f32>), |props, extent| props.extent(extent))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .children(Children::new(|| {
                        Ok(vec![
                            sample::text("Filters")?,
                            sample::text("Tap outside the panel to close it.")?,
                        ])
                    }))
                    .build(),
            )?;
            Ok::<_, LayoutError>(Container::column(vec![box_item(reopen), panel])?)
        })
        .title("Overlays/Drawer")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(by_role(Role::Dialog).named("Drawer"))?;
            canvas.expect_text("Filters")?;
            canvas.click_outside(by_role(Role::Dialog))?;
            canvas.expect_absent(by_role(Role::Dialog))?;
            canvas.expect_no_text("Filters")?;
            canvas.expect_action("on_close", 1)?;
            canvas.click(by_role(Role::Button).named("Open the drawer"))?;
            canvas.expect_text("Filters")
        }),
    ]
}
