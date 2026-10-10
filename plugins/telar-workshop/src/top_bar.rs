//! The top bar: the project's name, the trigger of the command palette, the environment every canvas is shown in and the view the workshop shows.
//!
//! What it holds follows the window's width: the canvas settings fold into one menu where they would not fit in place, and a narrow window adds the button that opens the sidebar's drawer.

use std::rc::Rc;

use telar::{
    AlignItems, Children, Clip, ClippedItem, LayoutError, LayoutItem, LayoutStyle, Reactive,
    ReactiveList, RectStyle, Role, StyledContainer, Text, TextWrap, box_item,
};
use telar_components::{ButtonProps, IconButtonProps, button, icon_button};
use telar_devtools::WORKBENCH_GRID;
use telar_icons::icon;

use crate::keymap::{Action, Keymap};
use crate::state::WorkshopState;
use crate::strings::{self, FIND, PREVIEWS, WORKSHOP};
use crate::{canvas_toolbar, view_switch};

const HEIGHT: f32 = 40.0;
const GAP: f32 = WORKBENCH_GRID * 2.0;

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
            .gap(GAP),
        |_| RectStyle::default(),
        vec![
            leading(state, keymap, project_name)?,
            box_item(StyledContainer::new(
                LayoutStyle::new().flex_grow(1.0),
                |_| RectStyle::default(),
                Vec::new(),
            )?),
            canvas_settings(state)?,
            view_switch::view_switch(state)?,
        ],
    )?
    .role(Role::Banner);
    Ok(box_item(bar))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Leading {
    Sidebar,
    Project,
    Find,
}

/// What the bar starts with: the sidebar's opener while the sidebar is a drawer, the project's name, and the palette's trigger. The only part of the bar that gives up width, by cutting the name short.
fn leading(
    state: &WorkshopState,
    keymap: &Keymap,
    project_name: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let arrangement = state.arrangement();
    let (state, keymap) = (state.clone(), keymap.clone());
    let leading = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .min_width(0.0)
            .gap(GAP),
        move || {
            let opener = arrangement
                .get()
                .sidebar_in_drawer()
                .then_some(Leading::Sidebar);
            opener
                .into_iter()
                .chain([Leading::Project, Leading::Find])
                .collect()
        },
        |part: &Leading| *part,
        move |part| match part {
            Leading::Sidebar => sidebar_opener(&state),
            Leading::Project => project(project_name.clone()),
            Leading::Find => find_trigger(&keymap),
        },
    )?;
    Ok(box_item(leading))
}

fn project(name: Option<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::declaring(
        move || name.clone().unwrap_or_else(|| strings::text(WORKSHOP)),
        LayoutStyle::new().min_width(0.0),
        |text| text.with_font_weight(600).with_text_wrap(TextWrap::NoWrap),
    )?;
    Ok(box_item(ClippedItem::new(box_item(label), Clip::x())))
}

/// Opens the sidebar's drawer, as its shortcut does.
fn sidebar_opener(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let drawer = state.sidebar_drawer();
    button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(PREVIEWS)))
            .ghost(true)
            .on_press(Rc::new(move || drawer.set(true)))
            .build(),
        Children::default(),
    )
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

/// The canvas settings, in place where the bar has room for them and as one menu where it has not.
fn canvas_settings(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let arrangement = state.arrangement();
    let state = state.clone();
    let settings = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0),
        move || vec![arrangement.get().settings_in_menu()],
        |in_menu: &bool| *in_menu,
        move |in_menu| match in_menu {
            true => canvas_toolbar::environment_menu(&state),
            false => canvas_toolbar::environment(&state),
        },
    )?;
    Ok(box_item(settings))
}
