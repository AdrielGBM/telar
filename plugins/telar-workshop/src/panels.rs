//! The panels along the canvas: a strip of tabs over the panel it shows. Controls is the only panel yet; actions, interactions, accessibility, source and layout join the strip as they land.

mod controls;
mod editors;

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    AlignItems, BorderRadius, Children, LayoutError, LayoutItem, LayoutStyle, Reactive,
    ReactiveList, RectStyle, Role, ShapeStyle, StyledContainer, Text, box_item,
};
use telar_components::{ButtonProps, button};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens};

use crate::state::WorkshopState;
use crate::strings::{self, CONTROLS, RESET_ALL};

/// What fills the panels pane the shell sizes.
pub(crate) fn panels(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panels = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0),
        |_| RectStyle::default(),
        vec![header(state)?, divider()?, controls::controls(state)?],
    )?;
    Ok(box_item(panels))
}

fn header(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let strip = StyledContainer::new(
        LayoutStyle::new().flex_row().gap(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default(),
        vec![tab(CONTROLS)?],
    )?;
    let spacer = StyledContainer::new(
        LayoutStyle::new().flex_grow(1.0),
        |_| RectStyle::default(),
        Vec::new(),
    )?;
    let header = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default(),
        vec![box_item(strip), box_item(spacer), reset_all(state)?],
    )?;
    Ok(box_item(header))
}

fn tab(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let tab = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        |_| {
            RectStyle::default()
                .with_fill(use_workbench_tokens().selection)
                .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
        },
        vec![box_item(label)],
    )?
    .control(Role::Tab)
    .toggled(|| true);
    Ok(box_item(tab))
}

/// Puts every arg of the selected preview back to its default, offered only while one holds something else.
fn reset_all(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let edited = state.clone();
    let state = state.clone();
    let cell = ReactiveList::with_style(
        LayoutStyle::new().flex_row(),
        move || {
            edited
                .selected()
                .filter(|entry| edited.args(entry).states().iter().any(|arg| arg.edited))
                .into_iter()
                .collect()
        },
        |entry: &PreviewEntry| entry.id,
        move |entry| {
            let args = state.args(&entry);
            let on_press: Rc<dyn Fn()> = Rc::new(move || args.reset());
            button(
                ButtonProps::props()
                    .label(Reactive::of(|| strings::text(RESET_ALL)))
                    .ghost(true)
                    .on_press(on_press)
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(cell))
}

fn divider() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let divider = StyledContainer::new(
        LayoutStyle::new().height(1.0).flex_shrink(0.0),
        |_| RectStyle::default().with_fill(use_workbench_tokens().border_subtle),
        Vec::new(),
    )?;
    Ok(box_item(divider))
}

#[cfg(test)]
#[path = "panels_test.rs"]
mod tests;
