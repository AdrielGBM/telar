//! The actions panel: the calls the mounted preview's callbacks received, newest first, each with what it was called with, narrowed by a filter.

use telar::preview::{ActionCall, ActionLog};
use telar::{
    Accessible, AlignItems, Children, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, Memo, Reactive, ReactiveList, Role, Text, box_item, children,
    memo,
};
use telar_components::{BadgeProps, badge};
use telar_devtools::{workbench_fill, workbench_muted};

use super::action_row::{ActionRowProps, action_row};
use super::actions_tools::{ActionsToolsProps, actions_tools};
use crate::state::WorkshopState;
use crate::strings::{self, ACTIONS, NO_ACTIONS, NO_MATCHING_ACTIONS};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Shown {
    Nothing,
    NoMatches,
    Calls,
}

/// What fills the panel while its tab is the one shown.
pub(super) fn actions(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let log = state.actions();
    let filter = state.actions_filter();
    let listed = memo(move || matching(&log.calls(), &filter.get()));
    let body = ReactiveList::with_style(
        workbench_fill(),
        move || {
            let shown = match (log.last().is_some(), listed.with(Vec::is_empty)) {
                (false, _) => Shown::Nothing,
                (true, true) => Shown::NoMatches,
                (true, false) => Shown::Calls,
            };
            vec![shown]
        },
        |shown: &Shown| *shown,
        move |shown| match shown {
            Shown::Nothing => empty(NO_ACTIONS),
            Shown::NoMatches => empty(NO_MATCHING_ACTIONS),
            Shown::Calls => calls(listed),
        },
    )?;
    Ok(box_item(body))
}

/// The calls whose name or arguments contain `filter`, ignoring case, newest first. A blank filter keeps every call.
pub(super) fn matching(calls: &[ActionCall], filter: &str) -> Vec<ActionCall> {
    let filter = filter.trim().to_lowercase();
    calls
        .iter()
        .rev()
        .filter(|call| {
            filter.is_empty()
                || call.name.to_lowercase().contains(&filter)
                || call
                    .args
                    .iter()
                    .any(|arg| arg.to_lowercase().contains(&filter))
        })
        .cloned()
        .collect()
}

/// The filter field and the button that empties the log, for the end of the strip while this panel is shown.
pub(super) fn tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    actions_tools(
        ActionsToolsProps::props()
            .filter(state.actions_filter())
            .log(state.actions())
            .build(),
        Children::default(),
    )
}

/// How many calls `log` took since it was last emptied, for the panel's tab; nothing while there are none.
pub(super) fn count_badge(log: ActionLog) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = ReactiveList::with_style(
        LayoutStyle::row(),
        move || match log.count() {
            0 => Vec::new(),
            _ => vec![()],
        },
        |_: &()| (),
        move |()| {
            badge(
                BadgeProps::props()
                    .label(Reactive::of(move || log.count().to_string()))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(count))
}

fn calls(listed: Memo<Vec<ActionCall>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = ReactiveList::keyed(
        move || listed.get(),
        |call: &ActionCall| call.seq,
        |call| {
            action_row(
                ActionRowProps::props().call(call.peek()).build(),
                Children::default(),
            )
        },
    )?;
    let log = Container::column(children![rows])?
        .role(Role::Log)
        .a11y_label(|| strings::text(ACTIONS));
    let scroll = LayoutScrollArea::new(LayoutStyle::new().flex_grow(1.0), box_item(log))?
        .stop_label(|| strings::text(ACTIONS));
    Ok(box_item(scroll))
}

fn empty(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = Container::new(
        workbench_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        children![message],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}
