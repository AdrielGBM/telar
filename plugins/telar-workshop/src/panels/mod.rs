//! The panels along the canvas: a strip of tabs over the panel it shows, Controls, Actions, Interactions, Layout, Accessibility or Snapshot. Layout and Accessibility are the devtools' inspection panels over the mounted canvas, the same ones the overlay shows over a whole window. Source joins the strip as it lands.

telar::rsx_modules!();

mod actions;
mod controls;
mod editors;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, Children, Color, Container, LayoutError, LayoutItem, LayoutStyle, Memo,
    Reactive, ReactiveList, Rect, RectStyle, Role, RwSignal, ShapeStyle, Slots, StyledContainer,
    box_item, children, effect, memo, signal, track_layout,
};
use telar_components::{
    ButtonProps, ButtonVariant, IconButtonProps, ItemProps, Tab, button, icon_button, item,
    metrics, tab_list,
};
use telar_devtools::{
    WORKBENCH_GRID, accessibility_panel, inspection_toggles, inspector_panel, use_workbench_tokens,
    workbench_fill,
};
use telar_icons::icon;

pub(crate) use controls::arg_table;

use crate::more_menu::{Rows, more_button};
use crate::state::{PanelPosition, PanelTab, WorkshopState};
use crate::strings::{
    self, ACCESSIBILITY, ACTIONS, BACK_TO_LIVE, CONTROLS, FOLD_PANELS, HIDE_PANELS, INTERACTIONS,
    LAYOUT, MEASURE, MORE_PANEL_TOOLS, OUTLINE, PANELS, REPLAY, RESET_ALL, RUN_ON_MOUNT, SNAPSHOT,
    UNFOLD_PANELS,
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

const HEADER_PAD: f32 = WORKBENCH_GRID;
/// The strip of tabs, which is all of the panels that folding them leaves.
pub(crate) const HEADER_HEIGHT: f32 = WORKBENCH_GRID * 5.0;

/// The tabs, and what acts on the panel shown at the strip's end. Where the two do not fit side by side the tools fold behind one menu before the tabs give up any room, so the tabs only scroll once the tools have gone.
fn header(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = state.panel_tab();
    let laid = signal(None::<RwSignal<Rect>>);
    let tabs_width = signal(0.0_f32);
    let tools_widths = signal(HashMap::<PanelTab, f32>::new());
    // Against widths taken while nothing was folded, so folding cannot change the answer that folded.
    let folded = memo(move || {
        let Some(header) = laid.get() else {
            return false;
        };
        let tab = shown.get();
        let tools = tools_widths.with(|widths| widths.get(&tab).copied().unwrap_or(0.0));
        let fold = metrics::control_height() + WORKBENCH_GRID;
        let room = header.get().width - 2.0 * HEADER_PAD - WORKBENCH_GRID - fold;
        tools > 0.0 && tabs_width.get() + tools > room
    });
    let tabs_box = move || match folded.get() {
        true => LayoutStyle::row().flex_shrink(1.0).min_width(0.0),
        false => LayoutStyle::row().flex_shrink(0.0),
    };
    let tabs = Container::new(tabs_box(), vec![strip(state)?])?.styled_by(tabs_box);
    let tabs_rect = track_layout(tabs.layout_node()).expect("the tabs are laid out");
    effect(move || {
        let width = tabs_rect.get().width;
        if !folded.get() {
            tabs_width.set_if_changed(width);
        }
    });
    let spacer = Container::new(LayoutStyle::new().flex_grow(1.0), Vec::new())?;
    let header = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(HEADER_HEIGHT)
            .gap(WORKBENCH_GRID)
            .padding_horizontal(HEADER_PAD),
        vec![
            box_item(tabs),
            box_item(spacer),
            tools(state, folded, tools_widths)?,
            fold_button(state)?,
        ],
    )?;
    unfold_on_a_tab(state);
    laid.set(track_layout(header.layout_node()));
    Ok(box_item(header))
}

/// Folds the panels down to their strip of tabs, or unfolds them; with the panels on the right, where a strip of tabs has no room to stand on its own, it hides them instead.
fn fold_button(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (collapsed, position) = (state.panel_collapsed(), state.panel_position());
    let shape = move || (collapsed.get(), position.get());
    let button = ReactiveList::with_style(
        LayoutStyle::row().flex_shrink(0.0),
        move || vec![shape()],
        |shape: &(bool, PanelPosition)| *shape,
        move |(folded, position)| {
            let (icon, label) = match (position, folded) {
                (PanelPosition::Right, _) => (icon!("lucide:chevron-right"), HIDE_PANELS),
                (PanelPosition::Bottom, false) => (icon!("lucide:chevron-down"), FOLD_PANELS),
                (PanelPosition::Bottom, true) => (icon!("lucide:chevron-up"), UNFOLD_PANELS),
            };
            icon_button(
                IconButtonProps::props()
                    .icon(icon)
                    .label(Reactive::of(move || strings::text(label)))
                    .on_press(Rc::new(move || {
                        collapsed.update(|folded| *folded = !*folded)
                    }))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(button))
}

/// Unfolds the panels whenever a tab is chosen, the one already chosen included: choosing a tab is asking to see it.
fn unfold_on_a_tab(state: &WorkshopState) {
    let (tab, collapsed) = (state.panel_tab(), state.panel_collapsed());
    let first = Cell::new(true);
    effect(move || {
        let _ = tab.get();
        if !first.replace(false) {
            collapsed.set_if_changed(false);
        }
    });
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

/// What acts on the panel shown, at the end of the strip: in place, recording in `widths` how wide the tools of each panel stand, or folded behind one menu.
fn tools(
    state: &WorkshopState,
    folded: Memo<bool>,
    widths: RwSignal<HashMap<PanelTab, f32>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = state.panel_tab();
    let state = state.clone();
    let tools = ReactiveList::with_style(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0),
        move || vec![(shown.get(), folded.get())],
        |key: &(PanelTab, bool)| *key,
        move |(tab, folded)| match folded {
            true => match tool_rows(&state, tab) {
                Some(rows) => more_button(MORE_PANEL_TOOLS, rows),
                None => nothing(),
            },
            false => {
                let spread = Container::new(
                    LayoutStyle::row().flex_shrink(0.0),
                    vec![spread_tools(&state, tab)?],
                )?;
                let laid = track_layout(spread.layout_node()).expect("the tools are laid out");
                effect(move || {
                    let width = laid.get().width;
                    if widths.with(|widths| widths.get(&tab) != Some(&width)) {
                        widths.update(|widths| {
                            widths.insert(tab, width);
                        });
                    }
                });
                Ok(box_item(spread))
            }
        },
    )?;
    Ok(box_item(tools))
}

fn spread_tools(state: &WorkshopState, tab: PanelTab) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match tab {
        PanelTab::Controls => reset_all(state),
        PanelTab::Interactions => interactions::tools(state),
        PanelTab::Layout => inspection_toggles(state.inspection()),
        PanelTab::Actions | PanelTab::Accessibility | PanelTab::Snapshot => nothing(),
    }
}

/// The tools of panel `tab` as the rows of a menu, built as it opens; `None` for a panel with none on the strip.
fn tool_rows(state: &WorkshopState, tab: PanelTab) -> Option<Rows> {
    let state = state.clone();
    let rows: Rows = match tab {
        PanelTab::Controls => Rc::new(move || {
            let state = state.clone();
            Children::new(move || {
                let edited = state
                    .selected()
                    .map(|entry| state.args(&entry))
                    .filter(|args| args.states().iter().any(|arg| arg.edited));
                let mut slots = Slots::new();
                slots.push(
                    None,
                    action_row(RESET_ALL, edited.is_some(), move || {
                        if let Some(args) = &edited {
                            args.reset();
                        }
                    })?,
                );
                Ok(slots)
            })
        }),
        PanelTab::Interactions => Rc::new(move || {
            let state = state.clone();
            Children::new(move || {
                let interactions = state.interactions().clone();
                let mut slots = Slots::new();
                if interactions.picked().is_some() {
                    let live = interactions.clone();
                    slots.push(
                        None,
                        action_row(BACK_TO_LIVE, true, move || live.back_to_live())?,
                    );
                }
                slots.push(None, switch_row(RUN_ON_MOUNT, interactions.auto_run())?);
                let replaying = state.clone();
                slots.push(
                    None,
                    action_row(REPLAY, true, move || interactions::replay(&replaying))?,
                );
                Ok(slots)
            })
        }),
        PanelTab::Layout => Rc::new(move || {
            let inspection = state.inspection();
            Children::new(move || {
                let mut slots = Slots::new();
                slots.push(None, switch_row(OUTLINE, inspection.outline())?);
                slots.push(None, switch_row(MEASURE, inspection.measure())?);
                Ok(slots)
            })
        }),
        PanelTab::Actions | PanelTab::Accessibility | PanelTab::Snapshot => return None,
    };
    Some(rows)
}

fn action_row(
    label: &'static str,
    enabled: bool,
    act: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    item(
        ItemProps::props()
            .label(Reactive::of(move || strings::text(label)))
            .disabled(Reactive::of(move || !enabled))
            .on_press(Rc::new(act))
            .build(),
        Children::default(),
    )
}

fn switch_row(label: &'static str, on: RwSignal<bool>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    item(
        ItemProps::props()
            .label(Reactive::of(move || strings::text(label)))
            .switch(Reactive::of(move || on.get()))
            .on_press(Rc::new(move || on.update(|on| *on = !*on)))
            .build(),
        Children::default(),
    )
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
