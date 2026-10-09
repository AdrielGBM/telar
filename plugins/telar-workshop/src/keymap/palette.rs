//! The command palette: every preview by its title and name, then every action with its shortcut, found by typing part of either.

use std::rc::Rc;

use telar::{Children, LayoutError, LayoutItem, Reactive};
use telar_components::{Command, CommandPaletteProps, command_palette};

use super::{ACTIONS, Action, Keymap, preview_command};
use crate::sidebar::listed;
use crate::state::WorkshopState;
use crate::strings::{self, FIND, PREVIEWS};

pub(super) fn palette(keymap: &Keymap) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listing = keymap.state.clone();
    let running = keymap.clone();
    command_palette(
        CommandPaletteProps::props()
            .open(keymap.palette)
            .commands(Reactive::of(move || commands(&listing)))
            .placeholder(Reactive::of(|| strings::text(FIND)))
            .on_run(Rc::new(move |command: &Command| {
                running.run_command(&command.id)
            }))
            .build(),
        Children::default(),
    )
}

/// What the palette lists: the previews in the order the sidebar lists them, then the actions but the one that opens it.
pub(super) fn commands(state: &WorkshopState) -> Vec<Command> {
    let group = strings::text(PREVIEWS);
    let previews = listed(state.entries(), "").into_iter().map(|entry| {
        Command::new(
            preview_command(&entry),
            format!("{} / {}", entry.title, entry.name),
        )
        .in_group(group.as_str())
        .with_keywords(
            entry
                .tags
                .iter()
                .copied()
                .chain([entry.component, entry.id]),
        )
    });
    let actions = ACTIONS
        .into_iter()
        .filter(|action| *action != Action::Find)
        .map(|action| {
            let command = Command::new(action.id(), strings::text(action.label()))
                .in_group(strings::text(action.group()));
            match action.chords().first() {
                Some(chord) => command.with_shortcut(*chord),
                None => command,
            }
        });
    previews.chain(actions).collect()
}
