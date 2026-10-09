//! The shortcuts overlay `?` opens: every action that has a shortcut, under its group, with its keys on caps.

use telar::{
    AlignItems, Children, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Reactive,
    RectStyle, Slots, StyledContainer, Text, box_item,
};
use telar_components::{KbdProps, ModalProps, kbd, modal};
use telar_devtools::{WORKBENCH_GRID, workbench_muted};

use super::{ACTIONS, Action, GROUPS, Keymap};
use crate::strings::{self, KEYBOARD_SHORTCUTS};

const WIDTH: f32 = WORKBENCH_GRID * 52.0;
const MAX_HEIGHT: f32 = WORKBENCH_GRID * 60.0;

pub(super) fn shortcuts(keymap: &Keymap) -> Result<Box<dyn LayoutItem>, LayoutError> {
    modal(
        ModalProps::props()
            .open(keymap.shortcuts)
            .title(Reactive::of(|| strings::text(KEYBOARD_SHORTCUTS)))
            .build(),
        Children::new(|| {
            let mut slots = Slots::new();
            slots.push(None, listing()?);
            Ok(slots)
        }),
    )
}

fn listing() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut sections = Vec::new();
    for group in GROUPS {
        let actions: Vec<Action> = ACTIONS
            .into_iter()
            .filter(|action| action.group() == group && !action.chords().is_empty())
            .collect();
        if actions.is_empty() {
            continue;
        }
        sections.push(heading(group)?);
        for action in actions {
            sections.push(row(action)?);
        }
    }
    let column = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(WIDTH)
            .gap(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default(),
        sections,
    )?;
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().flex_column().max_height(MAX_HEIGHT),
        box_item(column),
    )?;
    Ok(box_item(scroll))
}

fn heading(group: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || strings::text(group),
        LayoutStyle::new().padding_top(WORKBENCH_GRID),
        |text| workbench_muted(text.with_font_weight(600)),
    )?;
    Ok(box_item(text))
}

/// The action's name, then a cap per key of each chord that runs it.
fn row(action: Action) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = Text::declaring(
        move || strings::text(action.label()),
        LayoutStyle::new().flex_grow(1.0).min_width(0.0),
        |text| text,
    )?;
    let mut items = vec![box_item(name)];
    for chord in action.chords() {
        items.push(kbd(
            KbdProps::props()
                .chord(Reactive::of(move || chord.to_string()))
                .build(),
            Children::default(),
        )?);
    }
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        items,
    )?;
    Ok(box_item(row))
}
