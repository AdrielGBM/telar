use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{
    Children, Container, LayoutError, LayoutItem, LayoutStyle, ReactiveList, Role, box_item,
};

use crate::button::{ButtonProps, button};
use crate::context_menu::{ContextMenuProps, Entry, context_menu};

fn entries() -> Vec<Entry> {
    vec![
        Entry::row("Cut", "Ctrl+X", || {}),
        Entry::row("Copy", "Ctrl+C", || {}),
        Entry::row("Paste", "Ctrl+V", || {}).disabled(),
        Entry::Separator,
        Entry::Sub {
            label: "Share".into(),
            entries: vec![
                Entry::row("Copy link", "", || {}),
                Entry::row("Email", "", || {}),
            ],
        },
    ]
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(context_menu: ContextMenuProps, "Open", |p| {
            let open = p.signal("open", true);
            let width = p.arg("width", 180.0f32);
            let reopen = button(
                ButtonProps::props()
                    .label("Show the menu")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
                Children::default(),
            )?;
            let menu = ReactiveList::new(
                move || vec![open.get()],
                |shown: &bool| *shown,
                move |shown| -> Result<Box<dyn LayoutItem>, LayoutError> {
                    if !shown {
                        return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
                    }
                    context_menu(
                        ContextMenuProps::props()
                            .at((24.0, 64.0))
                            .entries(entries())
                            .width(width)
                            .on_close(Rc::new(move || open.set(false)))
                            .build(),
                        Children::default(),
                    )
                },
                0.0,
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(reopen), box_item(menu)],
            )?)
        })
        .title("Overlays/Context menu")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(by_role(Role::MenuItem).containing("Cut"))?;
            canvas.expect(by_role(Role::MenuItem).containing("Paste"))?;
            canvas.expect_expanded(by_role(Role::MenuItem).containing("Share"), false)?;
            canvas.click(by_role(Role::MenuItem).containing("Copy"))?;
            canvas.expect_no_text("Cut")?;
            canvas.click(by_role(Role::Button).named("Show the menu"))?;
            canvas.expect_text("Cut")
        }),
    ]
}
