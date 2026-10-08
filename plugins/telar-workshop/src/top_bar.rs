//! The top bar: the project's name, the search trigger and the view the workshop shows.

use std::rc::Rc;

use telar::{
    AlignItems, Children, Key, LayoutError, LayoutItem, LayoutStyle, Reactive, RectStyle, Role,
    StyledContainer, Text, box_item,
};
use telar_components::{IconButtonProps, icon_button};
use telar_devtools::WORKBENCH_GRID;
use telar_icons::icon;

use crate::state::{ViewMode, WorkshopState};
use crate::strings::{self, SEARCH, VIEW_CANVAS, VIEW_DOCS, VIEW_MATRIX, WORKSHOP};

const HEIGHT: f32 = 40.0;
const SEARCH_SHORTCUT: char = '/';

pub(crate) fn top_bar(
    state: &WorkshopState,
    project_name: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shortcut = state.clone();
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
            search_trigger(state)?,
            box_item(StyledContainer::new(
                LayoutStyle::new().flex_grow(1.0),
                |_| RectStyle::default(),
                Vec::new(),
            )?),
            view_label(state)?,
        ],
    )?
    .role(Role::Banner)
    .on_key(move |key: &Key| {
        let modifiers = telar::modifiers();
        let summoned =
            *key == Key::Char(SEARCH_SHORTCUT) && !modifiers.is_ctrl && !modifiers.is_meta;
        if summoned {
            shortcut.focus_search();
        }
        summoned
    });
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

/// Takes the keyboard to the sidebar's search field, as the shortcut its tooltip names does from anywhere a field is not taking text.
fn search_trigger(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    icon_button(
        IconButtonProps::props()
            .icon(icon!("lucide:search"))
            .label(Reactive::of(|| strings::text(SEARCH)))
            .shortcut(SEARCH_SHORTCUT.to_string())
            .on_press(Rc::new(move || state.focus_search()))
            .build(),
        Children::default(),
    )
}

fn view_label(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let view = state.view();
    let label = Text::declaring(
        move || strings::text(view_key(view.get())),
        LayoutStyle::new(),
        |text| text,
    )?;
    Ok(box_item(label))
}

fn view_key(view: ViewMode) -> &'static str {
    match view {
        ViewMode::Canvas => VIEW_CANVAS,
        ViewMode::Matrix => VIEW_MATRIX,
        ViewMode::Docs => VIEW_DOCS,
    }
}
