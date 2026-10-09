use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutStyle, box_item};

use crate::button::{ButtonProps, button};
use crate::command_palette::{Command, CommandPaletteProps, command_palette};

fn commands() -> Vec<Command> {
    vec![
        Command::new("open", "Open file")
            .in_group("File")
            .with_shortcut("Mod+O"),
        Command::new("save", "Save")
            .in_group("File")
            .with_shortcut("Mod+S"),
        Command::new("save-as", "Save as…")
            .in_group("File")
            .with_shortcut("Mod+Shift+S"),
        Command::new("theme", "Toggle dark mode")
            .in_group("View")
            .with_keywords(["theme", "appearance"]),
        Command::new("zoom-in", "Zoom in")
            .in_group("View")
            .with_shortcut("Mod+="),
        Command::new("zoom-out", "Zoom out")
            .in_group("View")
            .with_shortcut("Mod+-"),
        Command::new("sidebar", "Toggle sidebar")
            .in_group("View")
            .with_shortcut("Alt+S"),
        Command::new("print", "Print")
            .in_group("File")
            .disabled(true),
        Command::new("about", "About"),
    ]
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(command_palette: CommandPaletteProps, "Open", |p| {
            let open = p.signal("open", true);
            let reopen = button(
                ButtonProps::props()
                    .label("Open the palette")
                    .on_press(Rc::new(move || open.set(true)))
                    .build(),
                Children::default(),
            )?;
            let palette = command_palette(
                CommandPaletteProps::props()
                    .open(open)
                    .commands(commands())
                    .width(p.arg("width", 0.0f32))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(reopen), palette],
            )?)
        })
        .title("Overlays/Command palette")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
