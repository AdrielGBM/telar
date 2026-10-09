//! The actions panel: the calls the mounted preview's callbacks received, newest first, each with what it was called with, narrowed by a filter.

use std::rc::Rc;

use telar::preview::{ActionCall, ActionLog};
use telar::{
    Accessible, AlignItems, Border, Children, JustifyContent, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, Memo, Reactive, ReactiveList, RectStyle, Role, StyledContainer,
    Text, box_item, memo,
};
use telar_components::{BadgeProps, ButtonProps, TextFieldProps, badge, button, text_field};
use telar_devtools::{
    WORKBENCH_GRID, use_workbench_tokens, workbench_fill, workbench_mono, workbench_muted,
};

use crate::state::WorkshopState;
use crate::strings::{
    self, ACTIONS, CLEAR_ACTIONS, FILTER_ACTIONS, NO_ACTIONS, NO_MATCHING_ACTIONS,
};

const SEQ_WIDTH: f32 = 48.0;
const NAME_WIDTH: f32 = 160.0;
const FILTER_WIDTH: f32 = 200.0;

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
    let filter = text_field(
        TextFieldProps::props()
            .value(state.actions_filter())
            .placeholder(Reactive::of(|| strings::text(FILTER_ACTIONS)))
            .width(FILTER_WIDTH)
            .build(),
        Children::default(),
    )?;
    let log = state.actions();
    let clear = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(CLEAR_ACTIONS)))
            .ghost(true)
            .on_press(Rc::new(move || log.clear()))
            .build(),
        Children::default(),
    )?;
    let tools = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        vec![filter, clear],
    )?;
    Ok(box_item(tools))
}

/// How many calls `log` took since it was last emptied, for the panel's tab; nothing while there are none.
pub(super) fn count_badge(log: ActionLog) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = ReactiveList::with_style(
        LayoutStyle::new().flex_row(),
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
        |call| row(&call.peek()),
    )?;
    let log = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| RectStyle::default(),
        vec![box_item(rows)],
    )?
    .role(Role::Log)
    .a11y_label(|| strings::text(ACTIONS));
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().flex_grow(1.0).min_height(0.0),
        box_item(log),
    )?;
    Ok(box_item(scroll))
}

fn row(call: &ActionCall) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let seq = format!("#{}", call.seq);
    let seq = Text::declaring(
        move || seq.clone(),
        LayoutStyle::new().width(SEQ_WIDTH).flex_shrink(0.0),
        |text| workbench_muted(workbench_mono(text)),
    )?;
    let name = call.name;
    let name = Text::declaring(
        move || name.to_string(),
        LayoutStyle::new().width(NAME_WIDTH).flex_shrink(0.0),
        |text| text.with_font_weight(600),
    )?;
    let payload = call.args.join(", ");
    let payload = Text::declaring(
        move || payload.clone(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_basis(0.0)
            .min_width(0.0),
        workbench_mono,
    )?;
    let line = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 2.0)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID),
        |_| {
            RectStyle::default().with_border(Border::per_side(
                use_workbench_tokens().border_subtle,
                0.0,
                0.0,
                1.0,
                0.0,
            ))
        },
        vec![box_item(seq), box_item(name), box_item(payload)],
    )?;
    Ok(box_item(line))
}

fn empty(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = StyledContainer::new(
        workbench_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        vec![box_item(message)],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}
