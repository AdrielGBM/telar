use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, signal};

use super::sample;
use crate::icon_button::{IconButtonProps, icon_button};
use crate::toolbar::{ToolbarProps, toolbar};

fn tools() -> Children {
    sample::children(|| {
        let playing = signal(false);
        Ok(vec![
            icon_button(
                IconButtonProps::props()
                    .icon(telar_icons::icon!("lucide:play"))
                    .label("Run")
                    .shortcut("F5")
                    .pressed(playing)
                    .on_press(Rc::new(move || playing.set(true)))
                    .build(),
                Children::default(),
            )?,
            icon_button(
                IconButtonProps::props()
                    .icon(telar_icons::icon!("lucide:square"))
                    .label("Stop")
                    .shortcut("Shift+F5")
                    .on_press(Rc::new(move || playing.set(false)))
                    .build(),
                Children::default(),
            )?,
            icon_button(
                IconButtonProps::props()
                    .icon(telar_icons::icon!("lucide:rotate-cw"))
                    .label("Restart")
                    .build(),
                Children::default(),
            )?,
            icon_button(
                IconButtonProps::props()
                    .icon(telar_icons::icon!("lucide:settings"))
                    .label("Settings")
                    .disabled(true)
                    .build(),
                Children::default(),
            )?,
        ])
    })
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(toolbar: ToolbarProps, "Default", |p| {
            toolbar(
                ToolbarProps::props()
                    .label(p.arg("label", "Debug"))
                    .vertical(p.arg("vertical", false))
                    .build(),
                tools(),
            )
        })
        .title("Workbench/Toolbar")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
