//! The inspector drawer: the shared inspection panels over the whole window, the components with their box model or what a reader is told, beside the part of the app it leaves showing.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, Children, Color, Container, LayoutError, LayoutItem,
    LayoutStyle, Reactive, ReactiveList, RectStyle, Role, ShapeStyle, StyledContainer, Text,
    box_item, children,
};
use telar_components::{
    ButtonProps, ButtonVariant, SplitPaneProps, Tab, button, split_pane, tab_list,
};

use super::{DrawerTab, Model, banner, mounted_while, stats};
use crate::panels::{accessibility_panel, inspection_toggles, inspector_panel, panes};
use crate::strings::{self, ACCESSIBILITY, CLOSE, COMPONENTS, INSPECTOR, NODES, RESIZE_INSPECTOR};
use crate::workbench::{WORKBENCH_GRID, use_workbench_tokens, workbench_fill, workbench_muted};

const DRAWER_MIN_WIDTH: f32 = 220.0;
const DRAWER_MAX_FRACTION: f32 = 0.6;

pub(super) fn drawer(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new().absolute_fill().flex_row(),
        move || model.inspector_open.get(),
        move || {
            split_pane(
                SplitPaneProps::props()
                    .size(model.drawer_width)
                    .min(DRAWER_MIN_WIDTH)
                    .max_fraction(DRAWER_MAX_FRACTION)
                    .collapsible(false)
                    .label(Reactive::of(|| strings::text(RESIZE_INSPECTOR)))
                    .build(),
                panes([pane(model)?, uncovered(model)?]),
            )
        },
    )
}

/// Where the drawer leaves the app showing, with the build-error banner at its top and the stats at its bottom. It claims no pointer itself, so a press there is the app's.
fn uncovered(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::column()
            .flex_grow(1.0)
            .padding_all(WORKBENCH_GRID),
        vec![
            banner::beside_drawer(model)?,
            spacer()?,
            stats::beside_drawer(model)?,
        ],
    )?))
}

fn pane(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        workbench_fill(),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default()
                .with_fill(tokens.panel_background)
                .with_border(Border::uniform(tokens.border_subtle, 1.0))
        },
        vec![header(model)?, strip(model)?, body(model)?],
    )?
    .input_opaque()
    .role(Role::Complementary)
    .a11y_label(|| strings::text(INSPECTOR));
    Ok(box_item(pane))
}

fn header(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        || strings::text(INSPECTOR),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let inspection = model.inspection;
    let count = Text::declaring(
        move || strings::text_with(NODES, &inspection.nodes().len().to_string()),
        LayoutStyle::new().flex_grow(1.0),
        workbench_muted,
    )?;
    let close = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(CLOSE)))
            .variant(ButtonVariant::Ghost)
            .on_press(Rc::new(move || model.inspector_open.set(false)))
            .build(),
        Children::default(),
    )?;
    let header = Container::new(
        LayoutStyle::row()
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        vec![box_item(title), box_item(count), close],
    )?;
    Ok(box_item(header))
}

fn strip(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tabs = tab_list(
        model.drawer_tab,
        vec![
            Tab::new(
                DrawerTab::Components,
                Reactive::of(|| strings::text(COMPONENTS)),
            ),
            Tab::new(
                DrawerTab::Accessibility,
                Reactive::of(|| strings::text(ACCESSIBILITY)),
            ),
        ],
        Color::TRANSPARENT.into(),
        Some(Reactive::of(|| strings::text(INSPECTOR))),
    )?;
    Ok(box_item(Container::new(
        LayoutStyle::row()
            .flex_shrink(0.0)
            .padding_horizontal(WORKBENCH_GRID),
        vec![tabs],
    )?))
}

fn body(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = model.drawer_tab;
    let inspection = model.inspection;
    let panel = ReactiveList::with_style(
        workbench_fill(),
        move || vec![shown.get()],
        |tab: &DrawerTab| *tab,
        move |tab| match tab {
            DrawerTab::Components => {
                let toggles = Container::new(
                    LayoutStyle::row()
                        .flex_shrink(0.0)
                        .padding_horizontal(WORKBENCH_GRID * 1.5)
                        .padding_vertical(WORKBENCH_GRID / 2.0),
                    vec![inspection_toggles(inspection)?],
                )?;
                Ok(box_item(Container::new(
                    workbench_fill(),
                    vec![box_item(toggles), inspector_panel(inspection)?],
                )?))
            }
            DrawerTab::Accessibility => accessibility_panel(inspection),
        },
    )?;
    let body = Container::new(workbench_fill(), children![panel])?
        .role(Role::TabPanel)
        .a11y_label(move || {
            strings::text(match shown.get() {
                DrawerTab::Components => COMPONENTS,
                DrawerTab::Accessibility => ACCESSIBILITY,
            })
        });
    Ok(box_item(body))
}

fn spacer() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_grow(1.0),
        Vec::new(),
    )?))
}
