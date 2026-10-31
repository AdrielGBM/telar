use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Key, NamedKey, Reactive, Role};

use crate::list::{ItemProps, item};
use crate::select::{SelectProps, select};

fn sizes() -> Children {
    Children::new(|| {
        let mut items = Vec::new();
        for label in ["Small", "Medium", "Large"] {
            let props = ItemProps::props()
                .label(Reactive::of(move || label.to_string()))
                .build();
            items.push(item(props)?);
        }
        Ok(items)
    })
}

/// More choices than a window holds, so the open list is cut to the room it has and scrolls.
fn options() -> Children {
    Children::new(|| {
        let mut items = Vec::new();
        for n in 1..=40 {
            let props = ItemProps::props()
                .label(Reactive::of(move || format!("Option {n}")))
                .build();
            items.push(item(props)?);
        }
        Ok(items)
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(select: SelectProps, "Default", |p| {
            select(
                SelectProps::props()
                    .color(p.arg("color", Color::TRANSPARENT))
                    .stretch(p.arg("stretch", false))
                    .children(sizes())
                    .build(),
            )
        })
        .title("Inputs/Select")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            let trigger = by_role(Role::ComboBox).named("Small");
            canvas.expect_expanded(trigger.clone(), false)?;
            canvas.click(trigger.clone())?;
            canvas.expect_expanded(trigger, true)?;
            canvas.expect_text("Large")?;
            canvas.click(by_role(Role::Option).named("Large"))?;
            canvas.expect_expanded(by_role(Role::ComboBox).named("Large"), false)?;
            canvas.expect_action("on_select", 1)?;
            canvas.click(by_role(Role::ComboBox).named("Large"))?;
            canvas.expect_toggled(by_role(Role::Option).named("Large"), true)
        }),
        preview!(select: SelectProps, "Bound selection", |p| {
            let selected = p.signal("selected", 1u32);
            select(
                SelectProps::props()
                    .selected(selected)
                    .children(sizes())
                    .build(),
            )
        })
        .title("Inputs/Select")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::ComboBox).named("Medium"))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect(by_role(Role::Option).named("Large"))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_expanded(by_role(Role::ComboBox).named("Large"), false)
        }),
        preview!(select: SelectProps, "Long list", |p| {
            let selected = p.signal("selected", 0u32);
            select(
                SelectProps::props()
                    .selected(selected)
                    .children(options())
                    .build(),
            )
        })
        .title("Inputs/Select")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::ComboBox).named("Option 1"))?;
            canvas.press(Key::Named(NamedKey::End))?;
            canvas.click(by_role(Role::Option).named("Option 40"))?;
            canvas.expect_expanded(by_role(Role::ComboBox).named("Option 40"), false)?;
            canvas.click(by_role(Role::ComboBox).named("Option 40"))?;
            canvas.expect_toggled(by_role(Role::Option).named("Option 40"), true)
        }),
    ]
}
