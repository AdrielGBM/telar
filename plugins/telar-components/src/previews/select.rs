use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Key, NamedKey, Reactive, Role, Slots};

use crate::list::{ItemProps, item};
use crate::select::{SelectProps, select};

fn sizes() -> Children {
    Children::new(|| {
        let mut slots = Slots::new();
        for label in ["Small", "Medium", "Large"] {
            let props = ItemProps::props()
                .label(Reactive::of(move || label.to_string()))
                .build();
            slots.push(None, item(props, Children::default())?);
        }
        Ok(slots)
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(select: SelectProps, "Default", |p| {
            select(
                SelectProps::props()
                    .color(p.arg("color", Color::TRANSPARENT))
                    .stretch(p.arg("stretch", false))
                    .build(),
                sizes(),
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
            canvas.click(by_role(Role::MenuItem).named("Large"))?;
            canvas.expect_expanded(by_role(Role::ComboBox).named("Large"), false)?;
            canvas.expect_action("on_select", 1)
        }),
        preview!(select: SelectProps, "Bound selection", |p| {
            let selected = p.signal("selected", 1u32);
            select(SelectProps::props().selected(selected).build(), sizes())
        })
        .title("Inputs/Select")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::ComboBox).named("Medium"))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect(by_role(Role::MenuItem).named("Large"))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_expanded(by_role(Role::ComboBox).named("Large"), false)
        }),
    ]
}
