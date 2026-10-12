//! The workshop's layout: the top bar over the sidebar, and the canvas area beside it — the canvas or the docs page, as the view says — with the panels along one of its edges, each pane resized by a splitter.
//!
//! The shell owns where each region sits, how big it is and what it is announced as, and how that changes with the window's width (see [`Arrangement`](crate::arrangement::Arrangement)): a narrow window shows the sidebar in a drawer and the panels below the canvas. What a region shows is its own module's.
//!
//! The workspace — the canvas area and the panels, each a region of it — is the main landmark, and it holds both splitters: the one between it and the sidebar too, so a reader finds nothing between the landmarks.

use telar::{
    Accessible, Children, Color, Key, LayoutError, LayoutItem, LayoutStyle, Movable, Reactive,
    ReactiveList, RectStyle, Role, ShapeStyle, SizeDimension, Slots, StyledContainer, box_item,
    effect,
};
use telar_components::{
    DrawerProps, SizedPane, SplitDirection, SplitPaneProps, SplitRegion, drawer, split_pane,
};
use telar_devtools::{use_workbench_tokens, workbench_fill};

use crate::keymap::Keymap;
use crate::state::{
    PANEL_MAX_FRACTION, PANEL_MIN_SIZE, PanelPosition, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
    ViewMode, WorkshopState,
};
use crate::strings::{self, PANELS, PREVIEWS, RESIZE_PANELS, RESIZE_SIDEBAR, WORKSPACE};
use crate::view_switch::view_key;
use crate::{canvas, docs, matrix, panels, sidebar, top_bar};

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
    let keymap = Keymap::new(state);
    let mut regions = vec![
        top_bar::top_bar(state, &keymap, project_name)?,
        box_item(divider),
        body(state)?,
    ];
    regions.extend(keymap.overlays()?);
    let root = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        |_| RectStyle::default().with_fill(use_workbench_tokens().panel_background),
        regions,
    )?
    .on_key(move |key: &Key| keymap.on_key(key));
    Ok(box_item(root))
}

/// Everything under the top bar, as the window's width arranges it: the sidebar as a pane beside the workspace, or in a drawer over it.
///
/// The regions are built once, here, and every arrangement only places them: crossing a breakpoint or moving the panels to another edge rebuilds the splits around them, and the canvas keeps its mounted preview, its play and everything shown in it.
fn body(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let arrangement = state.arrangement();
    let drawer_open = state.sidebar_drawer();
    effect(move || {
        if !arrangement.get().sidebar_in_drawer() {
            drawer_open.set_if_changed(false);
        }
    });
    let regions = Regions::new(state)?;
    let building = state.clone();
    let body = ReactiveList::with_style(
        workbench_fill(),
        move || vec![arrangement.get().sidebar_in_drawer()],
        |in_drawer: &bool| *in_drawer,
        move |in_drawer| match in_drawer {
            false => beside_sidebar(&building, &regions),
            true => under_drawer(&building, &regions),
        },
    )?;
    Ok(box_item(body))
}

/// What an arrangement places, each built once and moved into whichever arrangement is current.
#[derive(Clone)]
struct Regions {
    sidebar: Movable,
    canvas: Movable,
    panels: Movable,
}

impl Regions {
    fn new(state: &WorkshopState) -> Result<Self, LayoutError> {
        Ok(Self {
            sidebar: Movable::new(|| sidebar::sidebar(state))?,
            canvas: Movable::new(|| canvas_pane(state))?,
            panels: Movable::new(|| panels_pane(state))?,
        })
    }
}

fn place(region: &Movable) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(region.outlet(workbench_fill())?))
}

/// The sidebar and the workspace beside it, split at the sidebar's width.
fn beside_sidebar(
    state: &WorkshopState,
    regions: &Regions,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    split_pane(
        SplitPaneProps::props()
            .size(state.sidebar_width())
            .min(SIDEBAR_MIN_WIDTH)
            .max(SIDEBAR_MAX_WIDTH)
            .collapsed(state.sidebar_collapsed())
            .label(Reactive::of(|| strings::text(RESIZE_SIDEBAR)))
            .region(SplitRegion::new(Role::Main, strings::reactive(WORKSPACE)))
            .build(),
        panes([sidebar_pane(regions)?, workspace(state, regions)?]),
    )
}

fn sidebar_pane(regions: &Regions) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        workbench_fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().sidebar_background),
        vec![place(&regions.sidebar)?],
    )?
    .role(Role::Navigation)
    .a11y_label(|| strings::text(PREVIEWS));
    Ok(box_item(pane))
}

/// The workspace across the whole window with the panels below the canvas, wherever they were put: beside a canvas this narrow they would leave it no room. The sidebar opens over it, in a drawer as wide as the sidebar's pane.
fn under_drawer(
    state: &WorkshopState,
    regions: &Regions,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let width = state
        .sidebar_width()
        .peek()
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
    let sidebar = regions.sidebar.clone();
    let sidebar = Children::new(move || {
        let mut slots = Slots::new();
        slots.push(None, place(&sidebar)?);
        Ok(slots)
    });
    let drawer = drawer(
        DrawerProps::props()
            .open(state.sidebar_drawer())
            .width(width)
            .label(Reactive::of(|| strings::text(PREVIEWS)))
            .color(Reactive::of(|| -> Color {
                use_workbench_tokens().sidebar_background
            }))
            .build(),
        sidebar,
    )?;
    let workspace = StyledContainer::new(
        workbench_fill(),
        |_| RectStyle::default(),
        vec![split_workspace(state, regions, PanelPosition::Bottom)?],
    )?
    .role(Role::Main)
    .a11y_label(|| strings::text(WORKSPACE));
    let column = StyledContainer::new(
        workbench_fill(),
        |_| RectStyle::default(),
        vec![box_item(workspace), drawer],
    )?;
    Ok(box_item(column))
}

/// The canvas and the panels, stacked when the panels sit below and side by side when they sit to the right, with the panels keeping their size as the window changes.
fn workspace(state: &WorkshopState, regions: &Regions) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let position = state.panel_position();
    let building = state.clone();
    let regions = regions.clone();
    let workspace = ReactiveList::with_style(
        workbench_fill(),
        move || vec![position.get()],
        |position: &PanelPosition| *position,
        move |position| split_workspace(&building, &regions, position),
    )?;
    Ok(box_item(workspace))
}

fn split_workspace(
    state: &WorkshopState,
    regions: &Regions,
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
        panes([place(&regions.canvas)?, place(&regions.panels)?]),
    )
}

/// The canvas area: the selected preview on its canvas, its matrix, or its component's docs page, as the view says.
fn canvas_pane(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let view = state.view();
    let building = state.clone();
    let shown = ReactiveList::with_style(
        workbench_fill(),
        move || vec![view.get()],
        |view: &ViewMode| *view,
        move |view| match view {
            ViewMode::Canvas => canvas::canvas(&building),
            ViewMode::Matrix => matrix::matrix(&building),
            ViewMode::Docs => docs::docs(&building),
        },
    )?;
    let pane = StyledContainer::new(
        workbench_fill(),
        move |_| {
            let tokens = use_workbench_tokens();
            let fill = match view.get() {
                ViewMode::Docs => tokens.panel_background,
                _ => tokens.canvas_backdrop,
            };
            RectStyle::default().with_fill(fill)
        },
        vec![box_item(shown)],
    )?
    .role(Role::Section)
    .a11y_label(move || strings::text(view_key(view.get())));
    Ok(box_item(pane))
}

fn panels_pane(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        workbench_fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().panel_background),
        vec![panels::panels(state)?],
    )?
    .role(Role::Section)
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

#[cfg(test)]
#[path = "shell_test.rs"]
mod tests;
