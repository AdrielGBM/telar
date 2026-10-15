//! The panel itself: what the play of the selected preview came to, step by step, and what acts on it.

use std::cell::RefCell;
use std::rc::Rc;

use telar::focus::{self, FocusId};
use telar::preview::Frame;
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, JustifyContent, Key,
    LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, NamedKey, Reactive, ReactiveList,
    RectStyle, Role, ShapeStyle, StyledContainer, Text, box_item, children, use_theme_tokens,
};
use telar_components::{
    BadgeProps, ButtonProps, ButtonVariant, ToggleProps, badge, button, metrics, toggle,
};
use telar_devtools::{
    WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens, workbench_fill, workbench_mono,
    workbench_muted,
};

use super::frame::thumbnail;
use super::{Interactions, Run, Step, StepStatus, Verdict, replay};
use crate::state::WorkshopState;
use crate::strings::{
    self, BACK_TO_LIVE, NO_PLAY, NO_PLAY_HINT, NOTHING_SELECTED, PLAY_FAILED_AT, PLAY_NEEDS_CANVAS,
    PLAY_NOT_RUN, PLAY_STEPS, PLAY_STEPS_PASSED, REPLAY, RUN_ON_MOUNT, SHOW_CANVAS, STEP,
    STEP_FAILED, STEP_PASSED, STEP_PENDING,
};

const NUMBER_WIDTH: f32 = WORKBENCH_GRID * 3.0;
const MARK_WIDTH: f32 = WORKBENCH_GRID * 2.0;
const FAILED_EDGE: f32 = 3.0;
const MESSAGE_MAX_WIDTH: f32 = WORKBENCH_GRID * 90.0;

/// What the panel shows for the selected preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Shown {
    NothingSelected,
    NoPlay,
    NeedsCanvas,
    NotRun,
    /// The steps of the run on the canvas mounted as this.
    Steps(u64),
}

fn shown(state: &WorkshopState) -> Shown {
    let Some(entry) = state.selected() else {
        return Shown::NothingSelected;
    };
    if entry.play.is_none() {
        return Shown::NoPlay;
    }
    let interactions = state.interactions();
    if interactions.mounted() != Some(entry.id) {
        return Shown::NeedsCanvas;
    }
    match interactions.run().filter(|run| run.entry == entry.id) {
        Some(run) => Shown::Steps(run.mount),
        None => Shown::NotRun,
    }
}

/// What fills the panel while its tab is the one shown.
pub(crate) fn panel(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let watching = state.clone();
    let building = state.clone();
    let body = ReactiveList::with_style(
        workbench_fill(),
        move || vec![shown(&watching)],
        |shown: &Shown| *shown,
        move |shown| match shown {
            Shown::NothingSelected => message(NOTHING_SELECTED, None, None),
            Shown::NoPlay => message(NO_PLAY, Some(NO_PLAY_HINT), None),
            Shown::NeedsCanvas => {
                let state = building.clone();
                let show = Action {
                    label: SHOW_CANVAS,
                    on_press: Rc::new(move || replay(&state)),
                };
                message(PLAY_NEEDS_CANVAS, None, Some(show))
            }
            Shown::NotRun => message(PLAY_NOT_RUN, None, None),
            Shown::Steps(_) => steps(building.interactions()),
        },
    )?;
    Ok(box_item(body))
}

/// What acts on the play, for the end of the strip while this panel is shown: the way back to live while a step is shown, whether the play runs as its canvas mounts, and a replay.
pub(crate) fn tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let interactions = state.interactions().clone();
    let picked = interactions.clone();
    let live = ReactiveList::with_style(
        LayoutStyle::row(),
        move || picked.picked().map(|_| ()).into_iter().collect(),
        |_: &()| (),
        move |()| {
            let interactions = interactions.clone();
            button(
                ButtonProps::props()
                    .label(Reactive::of(|| strings::text(BACK_TO_LIVE)))
                    .variant(ButtonVariant::Ghost)
                    .on_press(Rc::new(move || interactions.back_to_live()))
                    .build(),
                Children::default(),
            )
        },
    )?;
    let auto_run = toggle(
        ToggleProps::props()
            .checked(state.interactions().auto_run())
            .label(Reactive::of(|| strings::text(RUN_ON_MOUNT)))
            .build(),
        Children::default(),
    )?;
    let replaying = state.clone();
    let replay = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(REPLAY)))
            .on_press(Rc::new(move || replay(&replaying)))
            .build(),
        Children::default(),
    )?;
    let tools = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 1.5),
        vec![box_item(live), auto_run, replay],
    )?;
    Ok(box_item(tools))
}

/// How the play of the selected preview went, for the panel's tab: how many steps it took, green when they all passed and red when one failed; nothing until it has run on the canvas.
pub(crate) fn tab_badge(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let watching = state.clone();
    let interactions = state.interactions().clone();
    let tally = ReactiveList::with_style(
        LayoutStyle::row(),
        move || match shown(&watching) {
            Shown::Steps(_) => interactions
                .run()
                .filter(|run| run.ran)
                .map(|run| (run.steps.len(), run.verdict()))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        },
        |tally: &(usize, Verdict)| *tally,
        |(count, verdict)| {
            badge(
                BadgeProps::props()
                    .label(Reactive::of(move || count.to_string()))
                    .color(Reactive::of(move || verdict_color(verdict)))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(tally))
}

struct Action {
    label: &'static str,
    on_press: Rc<dyn Fn()>,
}

fn message(
    key: &'static str,
    hint: Option<&'static str>,
    action: Option<Action>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = children![Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        workbench_muted,
    )?];
    if let Some(hint) = hint {
        items.push(box_item(Text::declaring(
            move || strings::text(hint),
            LayoutStyle::new().max_width(MESSAGE_MAX_WIDTH),
            |text| workbench_muted(workbench_mono(text)),
        )?));
    }
    if let Some(Action { label, on_press }) = action {
        items.push(button(
            ButtonProps::props()
                .label(Reactive::of(move || strings::text(label)))
                .on_press(on_press)
                .build(),
            Children::default(),
        )?);
    }
    let centred = Container::new(
        workbench_fill()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 2.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        items,
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

/// The run's verdict over its steps, oldest first.
fn steps(interactions: &Interactions) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = Rows::new(interactions.clone());
    let listing = Rc::clone(&rows);
    let building = Rc::clone(&rows);
    let list = ReactiveList::with_style(
        LayoutStyle::column(),
        move || listing.interactions.run().map(listed).unwrap_or_default(),
        |row: &Row| (row.mount, row.ran, row.index),
        move |row| step_row(&building, row),
    )?;
    let list = Container::column(children![list])?
        .role(Role::List)
        .a11y_label(|| strings::text(PLAY_STEPS));
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().flex_grow(1.0).min_height(0.0),
        box_item(list),
    )?
    .stop_label(|| strings::text(PLAY_STEPS));
    let column = Container::new(
        workbench_fill(),
        vec![summary(interactions)?, box_item(scroll)],
    )?;
    Ok(box_item(column))
}

/// Says how the run went, as it changes: a reader hears a replay's verdict without going to find it.
fn summary(interactions: &Interactions) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let interactions = interactions.clone();
    let reading = interactions.clone();
    let text = Text::declaring(
        move || {
            reading
                .run()
                .map(|run| verdict_text(&run))
                .unwrap_or_default()
        },
        LayoutStyle::new().min_width(0.0),
        move |text| {
            let color = interactions
                .run()
                .map(|run| verdict_color(run.verdict()))
                .unwrap_or_else(|| use_workbench_tokens().text_muted);
            text.with_font_weight(600).with_color(color)
        },
    )?;
    let line = Container::new(
        LayoutStyle::row()
            .flex_shrink(0.0)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID),
        children![text],
    )?
    .role(Role::Status);
    Ok(box_item(line))
}

fn verdict_text(run: &Run) -> String {
    match run.verdict() {
        Verdict::NotRun => strings::text(PLAY_NOT_RUN),
        Verdict::Passed => strings::text_with(PLAY_STEPS_PASSED, &run.steps.len().to_string()),
        Verdict::Failed(index) => strings::text_with_values(
            PLAY_FAILED_AT,
            &[
                ("step", &(index + 1).to_string()),
                ("total", &run.steps.len().to_string()),
            ],
        ),
    }
}

fn verdict_color(verdict: Verdict) -> Color {
    let tokens = use_theme_tokens();
    match verdict {
        Verdict::NotRun => use_workbench_tokens().text_muted,
        Verdict::Passed => tokens.success(),
        Verdict::Failed(_) => tokens.error(),
    }
}

/// The rows of the list, with the control each framed step answers through, so the arrows can take focus from one to the next.
struct Rows {
    interactions: Interactions,
    controls: RefCell<Vec<Option<FocusId>>>,
}

impl Rows {
    fn new(interactions: Interactions) -> Rc<Self> {
        Rc::new(Self {
            interactions,
            controls: RefCell::default(),
        })
    }

    fn built(&self, index: usize, control: Option<FocusId>) {
        let mut controls = self.controls.borrow_mut();
        if controls.len() <= index {
            controls.resize(index + 1, None);
        }
        controls[index] = control;
    }

    /// Picks the framed step at `index` and takes the keyboard to it.
    fn go_to(&self, index: usize) -> bool {
        let control = self.controls.borrow().get(index).copied().flatten();
        let Some(control) = control else {
            return false;
        };
        self.interactions.pick(Some(index));
        focus::request(control);
        true
    }

    fn on_key(&self, index: usize, key: &Key) -> bool {
        let steps = self
            .interactions
            .run
            .with(|run| run.as_ref().map_or(0, |run| run.steps.len()));
        let last = steps.saturating_sub(1);
        match key {
            Key::Named(NamedKey::ArrowDown) => self.go_to((index + 1).min(last)),
            Key::Named(NamedKey::ArrowUp) => self.go_to(index.saturating_sub(1)),
            Key::Named(NamedKey::Home) => self.go_to(0),
            Key::Named(NamedKey::End) => self.go_to(last),
            Key::Named(NamedKey::Escape) if self.interactions.picked().is_some() => {
                self.interactions.back_to_live();
                true
            }
            _ => false,
        }
    }
}

#[derive(Clone)]
struct Row {
    mount: u64,
    ran: bool,
    index: usize,
    step: Step,
}

fn listed(run: Run) -> Vec<Row> {
    run.steps
        .iter()
        .enumerate()
        .map(|(index, step)| Row {
            mount: run.mount,
            ran: run.ran,
            index,
            step: step.clone(),
        })
        .collect()
}

/// One step: a line with its number, how it went and what it did, and under it, for a step that failed, why and the frame it left. A step with a frame is a toggle that shows that frame on the canvas.
fn step_row(rows: &Rc<Rows>, row: Row) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Row { index, step, .. } = row;
    let failed = step.failed();
    let interactions = rows.interactions.clone();
    let picked = move || interactions.picked() == Some(index);
    let mut items = vec![line(rows, index, &step, picked.clone())?];
    if let StepStatus::Failed(message) = &step.status {
        let interactions = rows.interactions.clone();
        let pick = move || interactions.pick(Some(index));
        items.push(failure(index, message.clone(), step.frame.clone(), pick)?);
    }
    let row = StyledContainer::new(
        LayoutStyle::column()
            .gap(WORKBENCH_GRID / 2.0)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID),
        move |_| row_paint(picked(), failed),
        items,
    )?;
    Ok(box_item(row))
}

/// The step's line, which the keyboard and a reader reach it by.
fn line(
    rows: &Rc<Rows>,
    index: usize,
    step: &Step,
    picked: impl Fn() -> bool + Clone + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = head(index, step)?;
    let label = step_label(index, step);
    if step.frame.is_none() {
        rows.built(index, None);
        return Ok(box_item(line.role(Role::ListItem).a11y_label(label)));
    }
    let interactions = rows.interactions.clone();
    let keys = Rc::clone(rows);
    let line = line
        .control(Role::Button)
        .toggled(picked.clone())
        .a11y_label(label)
        .on_press(move || match picked() {
            true => interactions.back_to_live(),
            false => interactions.pick(Some(index)),
        })
        .on_focused_key(move |key: &Key| keys.on_key(index, key));
    rows.built(index, line.focus_id());
    Ok(box_item(line))
}

/// Why step `index` failed, under what it did, and the frame it left, which shows on the canvas when pressed.
fn failure(
    index: usize,
    message: String,
    frame: Option<Frame>,
    pick: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = children![Text::declaring(
        move || message.clone(),
        LayoutStyle::new().max_width(MESSAGE_MAX_WIDTH),
        |text| workbench_mono(text).with_color(use_theme_tokens().error()),
    )?];
    if let Some(frame) = frame {
        items.push(thumbnail(frame, index, pick)?);
    }
    let details = Container::new(
        LayoutStyle::column()
            .align_items(AlignItems::START)
            .gap(WORKBENCH_GRID)
            .padding_start(NUMBER_WIDTH + MARK_WIDTH + WORKBENCH_GRID * 2.0),
        items,
    )?;
    Ok(box_item(details))
}

fn head(index: usize, step: &Step) -> Result<StyledContainer, LayoutError> {
    let number = (index + 1).to_string();
    let number = Text::declaring(
        move || number.clone(),
        LayoutStyle::new().width(NUMBER_WIDTH).flex_shrink(0.0),
        |text| workbench_muted(workbench_mono(text)),
    )?;
    let shape = mark_of(&step.status);
    let status = step.status.clone();
    let mark = Container::new(
        LayoutStyle::new()
            .width(MARK_WIDTH)
            .flex_shrink(0.0)
            .flex_row()
            .align_items(AlignItems::CENTER),
        vec![metrics::mark(
            move || shape,
            move |_| status_color(&status).into(),
        )?],
    )?;
    let action = step.action.clone();
    let action = Text::declaring(
        move || action.clone(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_basis(0.0)
            .min_width(0.0),
        workbench_mono,
    )?;
    let head = StyledContainer::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        children![number, mark, action],
    )?;
    Ok(head)
}

fn mark_of(status: &StepStatus) -> metrics::Mark {
    match status {
        StepStatus::Pending => metrics::Mark::Ring,
        StepStatus::Passed => metrics::Mark::Check,
        StepStatus::Failed(_) => metrics::Mark::Close,
    }
}

fn status_color(status: &StepStatus) -> Color {
    let tokens = use_theme_tokens();
    match status {
        StepStatus::Pending => use_workbench_tokens().text_muted,
        StepStatus::Passed => tokens.success(),
        StepStatus::Failed(_) => tokens.error(),
    }
}

/// What a reader is told of step `index`: its number, what it did and how that went.
fn step_label(index: usize, step: &Step) -> impl Fn() -> String + Clone + 'static {
    let (action, status) = (step.action.clone(), step.status.clone());
    move || {
        let status = match &status {
            StepStatus::Pending => strings::text(STEP_PENDING),
            StepStatus::Passed => strings::text(STEP_PASSED),
            StepStatus::Failed(_) => strings::text(STEP_FAILED),
        };
        strings::text_with_values(
            STEP,
            &[
                ("step", &(index + 1).to_string()),
                ("action", &action),
                ("status", &status),
            ],
        )
    }
}

fn row_paint(picked: bool, failed: bool) -> RectStyle {
    let tokens = use_workbench_tokens();
    let error = use_theme_tokens().error();
    let style = RectStyle::default().with_radius(BorderRadius::all(WORKBENCH_RADIUS));
    let style = match (picked, failed) {
        (true, _) => style.with_fill(tokens.selection),
        (false, true) => style.with_fill(error.with_alpha(0.1)),
        (false, false) => style,
    };
    match failed {
        true => style.with_border(Border::per_side(error, 0.0, 0.0, 0.0, FAILED_EDGE)),
        false => style,
    }
}
