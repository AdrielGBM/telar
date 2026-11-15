use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use super::sample::Given;
use telar::{Chord, Container, Key, LayoutError, NamedKey, Role, box_item};

use crate::inputs::button::button::{ButtonProps, button};
use crate::overlays::command_palette::command_palette::{Command, CommandPaletteProps, command_palette};
use telar::ControlSize;

fn commands() -> Vec<Command> {
    vec![
        Command::new("open", "Open file")
            .in_group("File")
            .with_shortcut(primary('o')),
        Command::new("save", "Save")
            .in_group("File")
            .with_shortcut(primary('s')),
        Command::new("save-as", "Save as…")
            .in_group("File")
            .with_shortcut(primary('s').with_shift()),
        Command::new("theme", "Toggle dark mode")
            .in_group("View")
            .with_keywords(["theme", "appearance"]),
        Command::new("zoom-in", "Zoom in")
            .in_group("View")
            .with_shortcut(primary('=')),
        Command::new("zoom-out", "Zoom out")
            .in_group("View")
            .with_shortcut(primary('-')),
        Command::new("sidebar", "Toggle sidebar")
            .in_group("View")
            .with_shortcut(Chord::new(Key::Char('s')).with_alt()),
        Command::new("print", "Print")
            .in_group("File")
            .disabled(true),
        Command::new("about", "About"),
    ]
}

fn primary(key: char) -> Chord {
    Chord::new(Key::Char(key)).with_primary()
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
            )?;
            let palette = command_palette(
                CommandPaletteProps::props()
                    .on_run(Rc::new(|_| {}))
                    .control(open)
                    .commands(commands())
                    .given(p.arg("width", None::<f32>), |props, value| props.width(value))
                    .size(p.arg("size", ControlSize::Regular))
                    .build(),
            )?;
            Ok::<_, LayoutError>(Container::column(vec![box_item(reopen), palette])?)
        })
        .title("Overlays/Command palette")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.type_text(by_role(Role::TextInput), "zoom")?;
            canvas.expect(by_role(Role::Option).containing("Zoom in"))?;
            canvas.expect_absent(by_role(Role::Option).containing("Save"))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_absent(by_role(Role::Dialog))?;
            canvas.expect_action("on_run", 1)?;
            canvas.click(by_role(Role::Button).named("Open the palette"))?;
            canvas.type_text(by_role(Role::TextInput), "zoom")?;
            canvas.expect(by_role(Role::Option).containing("Zoom out"))
        }),
    ]
}
