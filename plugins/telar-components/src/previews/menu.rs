use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Key, NamedKey, Reactive, Role, RwSignal};

use super::sample;
use crate::list::{ItemProps, SeparatorProps, item, separator};
use crate::menu::{MenuProps, menu};
use crate::menu_panel::{SubmenuProps, submenu};

fn actions() -> Children {
    sample::children(|| {
        let row = |label: &'static str, shortcut: &'static str| {
            item(
                ItemProps::props().label(label).shortcut(shortcut).build(),
                Children::default(),
            )
        };
        Ok(vec![
            row("New file", "Mod+N")?,
            row("Open…", "Mod+O")?,
            separator(SeparatorProps::props().build(), Children::default())?,
            item(
                ItemProps::props().label("Close").disabled(true).build(),
                Children::default(),
            )?,
        ])
    })
}

const THEMES: [&str; 3] = ["System", "Light", "Dark"];

/// The choices of a set, the one `chosen` holds ticked.
fn choices(chosen: RwSignal<usize>) -> Children {
    sample::children(move || {
        (0..THEMES.len())
            .map(|at| {
                item(
                    ItemProps::props()
                        .label(THEMES[at])
                        .checked(Reactive::of(move || chosen.get() == at))
                        .radio(true)
                        .on_press(Rc::new(move || chosen.set(at)))
                        .build(),
                    Children::default(),
                )
            })
            .collect()
    })
}

fn view_menu(chosen: RwSignal<usize>, rulers: RwSignal<bool>) -> Children {
    sample::children(move || {
        Ok(vec![
            submenu(
                SubmenuProps::props()
                    .label("Theme")
                    .hint(Reactive::of(move || THEMES[chosen.get()].to_string()))
                    .build(),
                choices(chosen),
            )?,
            submenu(
                SubmenuProps::props().label("Zoom").build(),
                sample::children(|| {
                    Ok(vec![
                        item(
                            ItemProps::props()
                                .label("Zoom in")
                                .shortcut("Mod+=")
                                .build(),
                            Children::default(),
                        )?,
                        item(
                            ItemProps::props()
                                .label("Zoom out")
                                .shortcut("Mod+-")
                                .build(),
                            Children::default(),
                        )?,
                    ])
                }),
            )?,
            separator(SeparatorProps::props().build(), Children::default())?,
            item(
                ItemProps::props()
                    .label("Rulers")
                    .checked(Reactive::of(move || rulers.get()))
                    .on_press(Rc::new(move || rulers.set(!rulers.get())))
                    .build(),
                Children::default(),
            )?,
        ])
    })
}

fn settings_menu(
    chosen: RwSignal<usize>,
    motion: RwSignal<bool>,
    contrast: RwSignal<bool>,
) -> Children {
    sample::children(move || {
        let switch = |label: &'static str, on: RwSignal<bool>| {
            item(
                ItemProps::props()
                    .label(label)
                    .switch(Reactive::of(move || on.get()))
                    .on_press(Rc::new(move || on.set(!on.get())))
                    .build(),
                Children::default(),
            )
        };
        Ok(vec![
            submenu(
                SubmenuProps::props()
                    .label("Theme")
                    .hint(Reactive::of(move || THEMES[chosen.get()].to_string()))
                    .build(),
                choices(chosen),
            )?,
            separator(SeparatorProps::props().build(), Children::default())?,
            switch("Reduce motion", motion)?,
            switch("High contrast", contrast)?,
        ])
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(menu: MenuProps, "Default", |p| {
            menu(
                MenuProps::props()
                    .label(p.arg("label", "File"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .bordered(p.arg("bordered", false))
                    .caret(p.arg("caret", true))
                    .stretch(p.arg("stretch", false))
                    .build(),
                actions(),
            )
        })
        .title("Overlays/Menu")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            let file = by_role(Role::Button).named("File");
            canvas.expect_expanded(file.clone(), false)?;
            canvas.click(file.clone())?;
            canvas.expect_expanded(file.clone(), true)?;
            canvas.expect(by_role(Role::MenuItem).containing("New file"))?;
            canvas.click(by_role(Role::MenuItem).containing("Open"))?;
            canvas.expect_absent(by_role(Role::MenuItem))?;
            canvas.expect_expanded(file.clone(), false)?;
            canvas.expect_action("on_select", 1)?;
            canvas.click(file)?;
            canvas.expect(by_role(Role::MenuItem).containing("New file"))
        }),
        preview!(menu: MenuProps, "Submenus", |p| {
            let chosen = p.signal("theme", 0usize);
            let rulers = p.signal("rulers", true);
            menu(
                MenuProps::props().label(p.arg("label", "View")).build(),
                view_menu(chosen, rulers),
            )
        })
        .title("Overlays/Menu")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let view = by_role(Role::Button).named("View");
            let theme = by_role(Role::MenuItem).containing("Theme");
            canvas.tab()?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.expect_expanded(theme.clone(), false)?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_expanded(theme.clone(), true)?;
            canvas.expect_toggled(by_role(Role::MenuItemRadio).containing("System"), true)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_absent(by_role(Role::MenuItemRadio))?;
            canvas.expect_focused(view.clone())?;
            canvas.click(view.clone())?;
            canvas.hover(theme.clone())?;
            canvas.expect_expanded(theme.clone(), true)?;
            canvas.expect_toggled(by_role(Role::MenuItemRadio).containing("Dark"), true)?;
            canvas.click(by_role(Role::MenuItemRadio).containing("Light"))?;
            canvas.expect_absent(by_role(Role::MenuItemRadio))?;
            canvas.click(view)?;
            canvas.hover(theme)?;
            canvas.expect_toggled(by_role(Role::MenuItemRadio).containing("Light"), true)
        }),
        preview!(menu: MenuProps, "Switches", |p| {
            let chosen = p.signal("theme", 0usize);
            let motion = p.signal("motion", false);
            let contrast = p.signal("contrast", true);
            menu(
                MenuProps::props().label(p.arg("label", "Settings")).build(),
                settings_menu(chosen, motion, contrast),
            )
        })
        .title("Overlays/Menu")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let settings = by_role(Role::Button).named("Settings");
            let motion = by_role(Role::MenuItemCheckBox).containing("Reduce motion");
            canvas.click(settings)?;
            canvas.expect_toggled(motion.clone(), false)?;
            canvas.click(motion.clone())?;
            canvas.expect_toggled(motion.clone(), true)?;
            canvas.expect(by_role(Role::MenuItemCheckBox).containing("High contrast"))?;
            canvas.hover(motion.clone())?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_toggled(motion, false)
        }),
    ]
}
