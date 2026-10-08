use std::rc::Rc;

use telar::preview::{Layout, PreviewEntry, preview};
use telar::{Children, Color, Reactive};

use crate::button::{ButtonProps, button};

#[derive(Clone, Copy, telar::PreviewArg)]
enum Emphasis {
    Filled,
    Outline,
    Ghost,
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(button: ButtonProps, "Default", |p| {
            button(
                ButtonProps::props()
                    .label(p.arg("label", "Save"))
                    .fill(p.arg("fill", Color::TRANSPARENT))
                    .ghost(p.arg("ghost", false))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Button")
        .layout(Layout::Centered),
        preview!(button: ButtonProps, "Emphasis", |p| {
            let accent = p.arg("accent", Color::TRANSPARENT);
            let props = ButtonProps::props().label(p.arg("label", "Save"));
            let props = match p.arg("emphasis", Emphasis::Filled) {
                Emphasis::Filled => props.fill(accent),
                Emphasis::Outline => props.outline(accent),
                Emphasis::Ghost => props.ghost(true),
            };
            button(props.build(), Children::default())
        })
        .title("Inputs/Button")
        .layout(Layout::Centered),
        preview!(button: ButtonProps, "Counting presses", |p| {
            let presses = p.signal("presses", 0u32);
            button(
                ButtonProps::props()
                    .label(Reactive::of(move || format!("Pressed {} times", presses.get())))
                    .on_press(Rc::new(move || presses.update(|count| *count += 1)))
                    .build(),
                Children::default(),
            )
        })
        .title("Inputs/Button")
        .layout(Layout::Centered)
        .tags(&["stateful"]),
    ]
}
