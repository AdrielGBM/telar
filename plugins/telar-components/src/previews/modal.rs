use std::rc::Rc;

use telar::preview::{Layout, PreviewEntry, preview};
use telar::{
    Children, Color, Container, LayoutError, LayoutStyle, Slots, Text, box_item,
};

use crate::button::{ButtonProps, button};
use crate::modal::{ModalProps, modal};

fn body() -> Children {
    Children::new(|| {
        let text = Text::declaring(
            || "This dialog stays inside the canvas.".to_string(),
            LayoutStyle::new().height(20.0),
            |t| t,
        )?;
        let mut slots = Slots::new();
        slots.push(None, box_item(text));
        Ok(slots)
    })
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
                Children::default(),
            )?;
            let dialog = modal(
                ModalProps::props()
                    .open(open)
                    .title(p.arg("title", "Confirm"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                body(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column().gap(8.0),
                vec![box_item(reopen), dialog],
            )?)
        })
        .title("Overlays/Modal")
        .layout(Layout::Centered)
        .tags(&["stateful"]),
    ]
}
