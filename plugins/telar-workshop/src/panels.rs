//! The panels along the canvas: a strip of tabs over the panel it shows, Controls or Actions. Interactions, accessibility, source and layout join the strip as they land.

mod actions;
mod controls;
mod editors;

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, Children, Color, LayoutError, LayoutItem, LayoutStyle, Reactive,
    ReactiveList, RectStyle, Role, ShapeStyle, StyledContainer, box_item,
};
use telar_components::{ButtonProps, Tab, button, tab_list};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_fill};

pub(crate) use controls::arg_table;

use crate::state::{PanelTab, WorkshopState};
use crate::strings::{self, ACTIONS, CONTROLS, PANELS, RESET_ALL};

const TABS: [PanelTab; 2] = [PanelTab::Controls, PanelTab::Actions];

/// What fills the panels pane the shell sizes.
pub(crate) fn panels(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panels = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0),
        |_| RectStyle::default(),
        vec![header(state)?, divider()?, body(state)?],
    )?;
    Ok(box_item(panels))
}

fn header(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
            .gap(WORKBENCH_GRID)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default(),
        vec![strip(state)?, box_item(spacer), tools(state)?],
    )?;
    Ok(box_item(header))
}

fn strip(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tabs = TABS
        .into_iter()
        .map(|tab| {
            let named = Tab::new(tab, strings::reactive(tab_key(tab)));
            Ok(match tab {
                PanelTab::Actions => named.trailing(actions::count_badge(state.actions())?),
                PanelTab::Controls => named,
            })
        })
        .collect::<Result<Vec<_>, LayoutError>>()?;
    tab_list(
        state.panel_tab(),
        tabs,
        Color::TRANSPARENT.into(),
        Some(strings::reactive(PANELS)),
    )
}

fn tab_key(tab: PanelTab) -> &'static str {
    match tab {
        PanelTab::Controls => CONTROLS,
        PanelTab::Actions => ACTIONS,
    }
}

/// What acts on the panel shown, at the end of the strip.
fn tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = state.panel_tab();
    let state = state.clone();
    let tools = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER),
        move || vec![shown.get()],
        |tab: &PanelTab| *tab,
        move |tab| match tab {
            PanelTab::Controls => reset_all(&state),
            PanelTab::Actions => actions::tools(&state),
        },
    )?;
    Ok(box_item(tools))
}

fn body(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = state.panel_tab();
    let building = state.clone();
    let panel = ReactiveList::with_style(
        workbench_fill(),
        move || vec![shown.get()],
        |tab: &PanelTab| *tab,
        move |tab| match tab {
            PanelTab::Controls => controls::controls(&building),
            PanelTab::Actions => actions::actions(&building),
        },
    )?;
    let body = StyledContainer::new(
        workbench_fill(),
        |_| RectStyle::default(),
        vec![box_item(panel)],
    )?
    .role(Role::TabPanel)
    .a11y_label(move || strings::text(tab_key(shown.get())));
    Ok(box_item(body))
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
