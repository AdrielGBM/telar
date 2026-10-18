//! The panels along the canvas: a strip of tabs over the panel it shows, Controls, Actions, Interactions, Layout, Accessibility or Snapshot. Layout and Accessibility are the devtools' inspection panels over the mounted canvas, the same ones the overlay shows over a whole window. Source joins the strip as it lands.

telar::rsx_modules!();

mod actions;
mod controls;
mod editors;

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, Children, Color, Container, LayoutError, LayoutItem, LayoutStyle,
    Reactive, ReactiveList, RectStyle, Role, ShapeStyle, StyledContainer, box_item, children,
};
use telar_components::{ButtonProps, ButtonVariant, Tab, button, tab_list};
use telar_devtools::{
    WORKBENCH_GRID, accessibility_panel, inspection_toggles, inspector_panel, use_workbench_tokens,
    workbench_fill,
};

pub(crate) use controls::arg_table;

use crate::state::{PanelTab, WorkshopState};
use crate::strings::{
    self, ACCESSIBILITY, ACTIONS, CONTROLS, INTERACTIONS, LAYOUT, PANELS, RESET_ALL, SNAPSHOT,
};
use crate::{interactions, snapshot};

const TABS: [PanelTab; 6] = [
    PanelTab::Controls,
    PanelTab::Actions,
    PanelTab::Interactions,
    PanelTab::Layout,
    PanelTab::Accessibility,
    PanelTab::Snapshot,
];

/// What fills the panels pane the shell sizes.
pub(crate) fn panels(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panels = Container::new(
        LayoutStyle::column().flex_grow(1.0).min_height(0.0),
        vec![header(state)?, divider()?, body(state)?],
    )?;
    Ok(box_item(panels))
}

fn header(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let spacer = Container::new(LayoutStyle::new().flex_grow(1.0), Vec::new())?;
    let header = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .gap(WORKBENCH_GRID)
            .padding_horizontal(WORKBENCH_GRID)
            .padding_vertical(WORKBENCH_GRID / 2.0),
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
                PanelTab::Controls => named.trailing(controls::count_badge(state)?),
                PanelTab::Actions => named.trailing(actions::count_badge(state.actions())?),
                PanelTab::Interactions => named.trailing(interactions::tab_badge(state)?),
                PanelTab::Snapshot => named.trailing(snapshot::tab_badge(state)?),
                PanelTab::Layout | PanelTab::Accessibility => named,
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
        PanelTab::Interactions => INTERACTIONS,
        PanelTab::Layout => LAYOUT,
        PanelTab::Accessibility => ACCESSIBILITY,
        PanelTab::Snapshot => SNAPSHOT,
    }
}

/// What acts on the panel shown, at the end of the strip.
fn tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = state.panel_tab();
    let state = state.clone();
    let tools = ReactiveList::with_style(
        LayoutStyle::row().align_items(AlignItems::CENTER),
        move || vec![shown.get()],
        |tab: &PanelTab| *tab,
        move |tab| match tab {
            PanelTab::Controls => reset_all(&state),
            PanelTab::Actions => actions::tools(&state),
            PanelTab::Interactions => interactions::tools(&state),
            PanelTab::Layout => inspection_toggles(state.inspection()),
            PanelTab::Accessibility => nothing(),
            PanelTab::Snapshot => snapshot::tools(&state),
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
            PanelTab::Interactions => interactions::panel(&building),
            PanelTab::Layout => inspector_panel(building.inspection()),
            PanelTab::Accessibility => accessibility_panel(building.inspection()),
            PanelTab::Snapshot => snapshot::panel(&building),
        },
    )?;
    let body = Container::new(workbench_fill(), children![panel])?
        .role(Role::TabPanel)
        .a11y_label(move || strings::text(tab_key(shown.get())));
    Ok(box_item(body))
}

/// Puts every arg of the selected preview back to its default, offered only while one holds something else.
fn reset_all(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let edited = state.clone();
    let state = state.clone();
    let cell = ReactiveList::with_style(
        LayoutStyle::row(),
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
                    .variant(ButtonVariant::Ghost)
                    .on_press(on_press)
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(cell))
}

fn nothing() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
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
