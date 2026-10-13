use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{
    Children, Container, Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, Rect, ReactiveList,
    Role, RwSignal, box_item,
};

use super::sample;
use crate::button::{ButtonProps, button};
use crate::list::{ItemProps, SeparatorProps, item, separator};
use crate::menu::{ContextMenuProps, context_menu, focused_rect, is_context_menu_key};
use crate::menu_panel::{SubmenuProps, submenu};

fn rows() -> Children {
    sample::children(|| {
        let row = |label: &'static str, shortcut: &'static str| {
            item(
                ItemProps::props().label(label).shortcut(shortcut).build(),
                Children::default(),
            )
        };
        Ok(vec![
            row("Cut", "Mod+X")?,
            row("Copy", "Mod+C")?,
            item(
                ItemProps::props()
                    .label("Paste")
                    .shortcut("Mod+V")
                    .disabled(true)
                    .build(),
                Children::default(),
            )?,
            separator(SeparatorProps::props().build(), Children::default())?,
            submenu(
                SubmenuProps::props().label("Share").build(),
                sample::children(|| {
                    Ok(vec![
                        item(
                            ItemProps::props().label("Copy link").build(),
                            Children::default(),
                        )?,
                        item(ItemProps::props().label("Email").build(), Children::default())?,
                    ])
                }),
            )?,
        ])
    })
}

/// A context menu shown while `shown` holds, opened where `place` says.
fn shown_at(
    shown: impl Fn() -> bool + 'static,
    place: impl Fn() -> Place + 'static,
    close: Rc<dyn Fn()>,
) -> Result<ReactiveList, LayoutError> {
    ReactiveList::new(
        move || vec![shown()],
        |shown: &bool| *shown,
        move |shown| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !shown {
                return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
            }
            let props = ContextMenuProps::props().on_close(close.clone());
            let props = match place() {
                Place::Point(x, y) => props.at((x, y)),
                Place::Around(rect) => props.around(rect),
            };
            context_menu(props.build(), rows())
        },
        0.0,
    )
}

#[derive(Clone, Copy)]
enum Place {
    Point(f32, f32),
    Around(Rect),
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(context_menu: ContextMenuProps, "Open", |p| {
            let open = p.signal("open", true);
            let reopen = button(
                ButtonProps::props()
                    .label("Show the menu")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![
                    box_item(reopen),
                    box_item(shown_at(
                        move || open.get(),
                        || Place::Point(24.0, 64.0),
                        Rc::new(move || open.set(false)),
                    )?),
                ],
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
        preview!(context_menu: ContextMenuProps, "From the keyboard", |_p| {
            let shown: RwSignal<Option<Rect>> = telar::signal(None);
            let target = button(
                ButtonProps::props()
                    .label("Document")
                    .on_press(Rc::new(|| {}))
                    .build(),
                Children::default(),
            )?;
            let asks = telar::StyledContainer::new(
                LayoutStyle::new().flex_column(),
                |_| telar::RectStyle::default(),
                vec![target],
            )?
            .on_key(move |key: &Key| {
                let asked = is_context_menu_key(key) && shown.peek().is_none();
                if let (true, Some(rect)) = (asked, focused_rect()) {
                    shown.set(Some(rect));
                }
                asked
            });
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![
                    box_item(asks),
                    box_item(shown_at(
                        move || shown.get().is_some(),
                        move || Place::Around(shown.peek().unwrap_or_default()),
                        Rc::new(move || shown.set(None)),
                    )?),
                ],
            )?)
        })
        .title("Overlays/Context menu")
        .layout(Layout::Padded)
        .play(|canvas| {
            let document = by_role(Role::Button).named("Document");
            canvas.tab()?;
            canvas.expect_focused(document.clone())?;
            canvas.press(Key::Named(NamedKey::ContextMenu))?;
            canvas.expect_expanded(by_role(Role::MenuItem).containing("Share"), false)?;
            canvas.press(Key::Named(NamedKey::End))?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_expanded(by_role(Role::MenuItem).containing("Share"), true)?;
            canvas.press(Key::Named(NamedKey::Escape))?;
            canvas.expect_expanded(by_role(Role::MenuItem).containing("Share"), false)?;
            canvas.press(Key::Named(NamedKey::Escape))?;
            canvas.expect_absent(by_role(Role::MenuItem))?;
            canvas.expect_focused(document)
        }),
    ]
}
