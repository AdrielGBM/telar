use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Container, Key, LayoutError, LayoutStyle, NamedKey, Role, RwSignal, children};

use crate::inputs::button::button::{ButtonProps, button};
use crate::layout::list::list::{ItemProps, SeparatorProps, item, separator};
use crate::overlays::menu::menu::{
    ContextMenuProps, MenuPlace, context_menu, focused_rect, is_context_menu_key, menu_layer,
};
use crate::overlays::menu::menu_panel::{SubmenuProps, submenu};

fn rows() -> Children {
    Children::new(|| {
        let row = |label: &'static str, shortcut: &'static str| {
            item(ItemProps::props().label(label).shortcut(shortcut).build())
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
            )?,
            separator(SeparatorProps::props().build())?,
            submenu(
                SubmenuProps::props()
                    .label("Share")
                    .children(Children::new(|| {
                        Ok(vec![
                            item(ItemProps::props().label("Copy link").build())?,
                            item(ItemProps::props().label("Email").build())?,
                        ])
                    }))
                    .build(),
            )?,
        ])
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(context_menu: ContextMenuProps, "Open", |_p| {
            let open = telar::signal(Some(MenuPlace::At((24.0, 64.0))));
            let reopen = button(
                ButtonProps::props()
                    .label("Show the menu")
                    .on_press(Rc::new(move || open.set(Some(MenuPlace::At((24.0, 64.0))))))
                    .build(),
            )?;
            Ok::<_, LayoutError>(Container::column(children![
                reopen,
                menu_layer(open, |place| *place, |_| rows())?,
            ])?)
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
            let shown: RwSignal<Option<MenuPlace>> = telar::signal(None);
            let target = button(
                ButtonProps::props()
                    .label("Document")
                    .on_press(Rc::new(|| {}))
                    .build(),
            )?;
            let asks = telar::StyledContainer::new(
                LayoutStyle::column(),
                |_| telar::RectStyle::default(),
                vec![target],
            )?
            .on_key(move |key: &Key| {
                let asked = is_context_menu_key(key) && shown.peek().is_none();
                if let (true, Some(rect)) = (asked, focused_rect()) {
                    shown.set(Some(MenuPlace::Under(rect)));
                }
                asked
            });
            Ok::<_, LayoutError>(Container::column(children![
                asks,
                menu_layer(shown, |place| *place, |_| rows())?,
            ])?)
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
            canvas.expect_focused(document)?;
            canvas.press(Key::Named(NamedKey::ContextMenu))?;
            canvas.press(Key::Named(NamedKey::End))?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_expanded(by_role(Role::MenuItem).containing("Share"), true)
        }),
    ]
}
