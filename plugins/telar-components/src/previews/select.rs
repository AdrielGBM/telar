use telar::preview::{Layout, PreviewEntry, preview};
use telar::{Children, Color, Reactive, Slots};

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
        .layout(Layout::Centered),
        preview!(select: SelectProps, "Bound selection", |p| {
            let selected = p.signal("selected", 1u32);
            select(SelectProps::props().selected(selected).build(), sizes())
        })
        .title("Inputs/Select")
        .layout(Layout::Centered)
        .tags(&["stateful"]),
    ]
}
