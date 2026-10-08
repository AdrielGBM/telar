//! The workshop's layout: the top bar over the sidebar, and the canvas beside it with the panels along one of its edges, each pane resized by a splitter.
//!
//! The shell owns where each region sits, how big it is and what it is announced as. What a region shows is its own module's.

use telar::{
    Accessible, Children, LayoutError, LayoutItem, LayoutStyle, Reactive, ReactiveList, RectStyle,
    Role, ShapeStyle, SizeDimension, Slots, StyledContainer, box_item,
};
use telar_components::{SizedPane, SplitDirection, SplitPaneProps, split_pane};
use telar_devtools::use_workbench_tokens;

use crate::state::{
    PANEL_MAX_FRACTION, PANEL_MIN_SIZE, PanelPosition, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
    WorkshopState,
};
use crate::strings::{self, PANELS, PREVIEWS, RESIZE_PANELS, RESIZE_SIDEBAR, VIEW_CANVAS};
use crate::{canvas, panels, sidebar, top_bar};

const DIVIDER: f32 = 1.0;

pub(crate) fn shell(
    state: &WorkshopState,
    project_name: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let divider = StyledContainer::new(
        LayoutStyle::new().height(DIVIDER).flex_shrink(0.0),
        |_| RectStyle::default().with_fill(use_workbench_tokens().border_subtle),
        Vec::new(),
    )?;
    let root = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        |_| RectStyle::default().with_fill(use_workbench_tokens().panel_background),
        vec![
            top_bar::top_bar(state, project_name)?,
            box_item(divider),
            body(state)?,
        ],
    )?;
    Ok(box_item(root))
}

/// The sidebar and the workspace beside it, split at the sidebar's width.
fn body(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    split_pane(
        SplitPaneProps::props()
            .size(state.sidebar_width())
            .min(SIDEBAR_MIN_WIDTH)
            .max(SIDEBAR_MAX_WIDTH)
            .collapsed(state.sidebar_collapsed())
            .label(Reactive::of(|| strings::text(RESIZE_SIDEBAR)))
            .build(),
        panes([sidebar_pane(state)?, workspace(state)?]),
    )
}

fn sidebar_pane(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().sidebar_background),
        vec![sidebar::sidebar(state)?],
    )?
    .role(Role::Navigation)
    .a11y_label(|| strings::text(PREVIEWS));
    Ok(box_item(pane))
}

/// The canvas and the panels, stacked when the panels sit below and side by side when they sit to the right, with the panels keeping their size as the window changes.
fn workspace(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let position = state.panel_position();
    let building = state.clone();
    let workspace = ReactiveList::with_style(
        fill(),
        move || vec![position.get()],
        |position: &PanelPosition| *position,
        move |position| split_workspace(&building, position),
    )?;
    Ok(box_item(workspace))
}

fn split_workspace(
    state: &WorkshopState,
    position: PanelPosition,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let direction = match position {
        PanelPosition::Bottom => SplitDirection::Column,
        PanelPosition::Right => SplitDirection::Row,
    };
    split_pane(
        SplitPaneProps::props()
            .direction(direction)
            .sized(SizedPane::Second)
            .size(state.panel_size())
            .min(PANEL_MIN_SIZE)
            .max_fraction(PANEL_MAX_FRACTION)
            .collapsed(state.panel_collapsed())
            .label(Reactive::of(|| strings::text(RESIZE_PANELS)))
            .build(),
        panes([canvas_pane(state)?, panels_pane(state)?]),
    )
}

fn canvas_pane(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().canvas_backdrop),
        vec![canvas::canvas(state)?],
    )?
    .role(Role::Main)
    .a11y_label(|| strings::text(VIEW_CANVAS));
    Ok(box_item(pane))
}

fn panels_pane(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().panel_background),
        vec![panels::panels(state)?],
    )?
    .role(Role::Complementary)
    .a11y_label(|| strings::text(PANELS));
    Ok(box_item(pane))
}

fn panes(items: [Box<dyn LayoutItem>; 2]) -> Children {
    let mut slots = Slots::new();
    for item in items {
        slots.push(None, item);
    }
    Children::from(slots)
}

fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}
