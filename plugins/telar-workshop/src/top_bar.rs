//! The top bar: the project's name, the trigger of the command palette, the environment every canvas is shown in and the view the workshop shows.

use std::rc::Rc;

use telar::{
    AlignItems, Children, LayoutError, LayoutItem, LayoutStyle, Reactive, RectStyle, Role,
    StyledContainer, Text, box_item,
};
use telar_components::{IconButtonProps, icon_button};
use telar_devtools::WORKBENCH_GRID;
use telar_icons::icon;

use crate::keymap::{Action, Keymap};
use crate::state::WorkshopState;
use crate::strings::{self, FIND, WORKSHOP};
use crate::{canvas_toolbar, view_switch};

const HEIGHT: f32 = 40.0;

pub(crate) fn top_bar(
    state: &WorkshopState,
    keymap: &Keymap,
    project_name: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(HEIGHT)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .gap(WORKBENCH_GRID * 2.0),
        |_| RectStyle::default(),
        vec![
            project(project_name)?,
            find_trigger(keymap)?,
            box_item(StyledContainer::new(
                LayoutStyle::new().flex_grow(1.0),
                |_| RectStyle::default(),
                Vec::new(),
            )?),
            canvas_toolbar::environment(state)?,
            view_switch::view_switch(state)?,
        ],
    )?
    .role(Role::Banner);
    Ok(box_item(bar))
}

fn project(name: Option<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::declaring(
        move || name.clone().unwrap_or_else(|| strings::text(WORKSHOP)),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    Ok(box_item(label))
}

/// Opens the command palette, as the shortcut its tooltip names does from anywhere a field is not taking text.
fn find_trigger(keymap: &Keymap) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keymap = keymap.clone();
    icon_button(
        IconButtonProps::props()
            .icon(icon!("lucide:search"))
            .label(Reactive::of(|| strings::text(FIND)))
            .shortcut(Action::Find.chords()[0].to_string())
            .on_press(Rc::new(move || keymap.run(Action::Find)))
            .build(),
        Children::default(),
    )
}
