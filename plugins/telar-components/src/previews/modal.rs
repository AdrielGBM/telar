use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle, Role, Text, box_item};

use crate::button::{ButtonProps, button};
use crate::modal::{ModalProps, modal};

fn body_in(height: Option<f32>) -> Children {
    Children::one(move || {
        let text = Text::declaring(
            || "This dialog stays inside the canvas.".to_string(),
            height.map_or_else(LayoutStyle::new, |height| LayoutStyle::new().height(height)),
            |t| t,
        )?;
        Ok(box_item(text))
    })
}

fn long_body() -> Children {
    Children::new(|| {
        let mut items = Vec::new();
        for line in 0..40 {
            let text = Text::declaring(
                move || format!("Paragraph {line} of a body longer than the window."),
                LayoutStyle::new().height(20.0),
                |t| t,
            )?;
            items.push(box_item(text));
        }
        Ok(items)
    })
}

fn body() -> Children {
    body_in(Some(20.0))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(modal: ModalProps, "Open", |p| {
            let open = p.signal("open", true);
            let reopen = button(
                ButtonProps::props()
                    .label("Reopen")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
            )?;
            let dialog = modal(
                ModalProps::props()
                    .open(open)
                    .title(p.arg("title", "Confirm"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .children(body())
                    .build(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().gap(8.0),
                vec![box_item(reopen), dialog],
            )?)
        })
        .title("Overlays/Modal")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(by_role(Role::Dialog).named("Confirm"))?;
            canvas.expect_text("This dialog stays inside the canvas.")?;
            canvas.click(by_role(Role::Button).named("Close"))?;
            canvas.expect_absent(by_role(Role::Dialog))?;
            canvas.expect_no_text("This dialog stays inside the canvas.")?;
            canvas.expect_action("on_close", 1)?;
            canvas.click(by_role(Role::Button).named("Reopen"))?;
            canvas.expect(by_role(Role::Dialog).named("Confirm"))
        }),
        preview!(modal: ModalProps, "Long body", |p| {
            modal(
                ModalProps::props()
                    .open(p.signal("open", true))
                    .title("Terms")
                    .children(long_body())
                    .build(),
            )
        })
        .title("Overlays/Modal")
        .viewport(480.0, 360.0)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::Dialog).named("Terms"))?;
            canvas.expect(by_role(Role::Button).named("Close"))
        }),
        preview!(modal: ModalProps, "Narrow window", |p| {
            modal(
                ModalProps::props()
                    .open(p.signal("open", true))
                    .title("Confirm")
                    .children(body_in(None))
                    .build(),
            )
        })
        .title("Overlays/Modal")
        .viewport(240.0, 320.0)
        .play(|canvas| canvas.expect(by_role(Role::Dialog).named("Confirm"))),
    ]
}
