//! One cell of the matrix: its coordinates, what its play, its accessibility checks and its last snapshot came to, and the preview, mounted the first time the cell scrolls into view on a canvas of its own in the cell's environment, with its play run there.

use std::cell::RefCell;
use std::rc::Rc;

use telar::preview::a11y::Severity;
use telar::preview::snapshot::SnapshotStatus;
use telar::preview::{Globals, MatrixCell, Play, PreviewEntry};
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, ColorScheme, Container,
    Direction, LayoutError, LayoutItem, LayoutStyle, Lazy, Memo, PointerButton, Reactive,
    ReactiveList, RectStyle, RelayoutHook, Role, RwSignal, ShapeStyle, Size, StyledContainer,
    SurfaceFrame, Text, ViewRange, box_item, children, effect, memo, on_cleanup, pointer_position,
    scroll_progress, signal, use_theme_tokens,
};
use telar_components::{BadgeProps, ItemProps, TooltipProps, badge, item, tooltip};
use telar_devtools::{
    WORKBENCH_GRID, WORKBENCH_RADIUS, inspection_marks, use_workbench_tokens, workbench_mono,
    workbench_muted,
};

use super::open_cell;
use super::plan::cell_label;
use crate::canvas::inspected::PreviewOnly;
use crate::guarded::guarded;
use crate::more_menu::{Asked, menu_layer};
use crate::settings::CanvasSettings;
use crate::settle::run_when_sized;
use crate::state::WorkshopState;
use crate::strings::{
    self, A11Y_ERRORS, A11Y_PASSED, A11Y_WARNINGS, BUILD_FAILED, FOCUS_CELL, OPEN_IN_CANVAS,
    PLAY_FAILED, PLAY_PASSED, PLAY_SKIPPED, SNAPSHOT_DRIFTED, SNAPSHOT_MATCHES,
};

pub(super) const CELL_WIDTH: f32 = 480.0;
const CELL_MAX_HEIGHT: f32 = 360.0;
const PAD: f32 = WORKBENCH_GRID;
const FOCUSED_RING: f32 = 2.0;
/// The outer width of a cell, which every column heading takes too so the headings line up over the cells.
pub(super) const CELL_OUTER_WIDTH: f32 = CELL_WIDTH + 2.0 * PAD;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Status {
    BuildFailed(String),
    PlayPassed,
    PlayFailed(String),
    /// Not run: the cell changes args the play was written against.
    PlaySkipped,
    Accessible,
    A11yWarnings(usize),
    A11yErrors(usize),
    SnapshotMatches,
    SnapshotDrifted,
}

#[derive(Clone, Copy)]
enum Tone {
    Good,
    Fair,
    Bad,
    Quiet,
}

impl Status {
    fn text(&self) -> String {
        match self {
            Self::BuildFailed(_) => strings::text(BUILD_FAILED),
            Self::PlayPassed => strings::text(PLAY_PASSED),
            Self::PlayFailed(_) => strings::text(PLAY_FAILED),
            Self::PlaySkipped => strings::text(PLAY_SKIPPED),
            Self::Accessible => strings::text(A11Y_PASSED),
            Self::A11yWarnings(count) => strings::text_with(A11Y_WARNINGS, &count.to_string()),
            Self::A11yErrors(count) => strings::text_with(A11Y_ERRORS, &count.to_string()),
            Self::SnapshotMatches => strings::text(SNAPSHOT_MATCHES),
            Self::SnapshotDrifted => strings::text(SNAPSHOT_DRIFTED),
        }
    }

    fn tone(&self) -> Tone {
        match self {
            Self::PlayPassed | Self::Accessible | Self::SnapshotMatches => Tone::Good,
            Self::A11yWarnings(_) => Tone::Fair,
            Self::PlaySkipped => Tone::Quiet,
            Self::BuildFailed(_)
            | Self::PlayFailed(_)
            | Self::A11yErrors(_)
            | Self::SnapshotDrifted => Tone::Bad,
        }
    }

    fn detail(&self) -> Option<&str> {
        match self {
            Self::BuildFailed(message) | Self::PlayFailed(message) => Some(message),
            _ => None,
        }
    }

    /// What the last test run found of the cell's snapshot: a match, or one that failed the run.
    pub(super) fn of_snapshot(status: SnapshotStatus) -> Self {
        match status.is_failure() {
            false => Self::SnapshotMatches,
            true => Self::SnapshotDrifted,
        }
    }
}

/// Gives `globals`, already holding what the preview asks for, the toolbar's settings over it, its mode as the workshop's `scheme` resolves it, and then the cell's own; a canvas left with no direction reads in its locale's.
pub(crate) fn seed(
    globals: &Globals,
    settings: &CanvasSettings,
    scheme: ColorScheme,
    entry: &PreviewEntry,
    cell: &MatrixCell,
) {
    if let Some(mode) = settings.mode.mode(entry.env.mode, scheme) {
        globals.mode().set(Some(mode));
    }
    if let Some(locale) = &settings.locale {
        globals.locale().set(Some(locale.clone()));
    }
    if let Some(direction) = settings.direction {
        globals.direction().set(Some(direction));
    }
    if let Some(size) = settings.control_size {
        globals.control_size().set(Some(size));
    }
    if let Some(high) = settings.high_contrast {
        globals.high_contrast().set(Some(high));
    }
    globals
        .background()
        .set(settings.background.color(entry.env.background));
    cell.seed(globals);
    if globals.direction().peek().is_none()
        && let Some(locale) = globals.locale().peek()
    {
        globals
            .direction()
            .set(Some(Direction::for_locale(&locale)));
    }
}

/// The canvas a cell mounts on, and how far it is shrunk for display; where nothing names a viewport it is the size `cargo telar test` runs a preview at, so a live result is the one the report holds.
pub(super) fn sizing(entry: &PreviewEntry, cell: &MatrixCell) -> (Size, f32) {
    let viewport = cell
        .globals
        .viewport
        .or(entry.env.viewport)
        .unwrap_or(Play::DEFAULT_VIEWPORT);
    let scale = (CELL_WIDTH / viewport.width)
        .min(CELL_MAX_HEIGHT / viewport.height)
        .min(1.0);
    (viewport, scale)
}

pub(super) fn cell(
    state: &WorkshopState,
    entry: PreviewEntry,
    cell: MatrixCell,
    settings: Rc<CanvasSettings>,
    scheme: ColorScheme,
    plays: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = cell_label(&cell);
    let menu = cell_menu(state, entry, cell.clone())?;
    let focus = state.matrix_focus();
    let id = cell.id.clone();
    let focused = memo(move || focus.with(|focused| focused.as_deref() == Some(id.as_str())));
    let focus_this = {
        let id = cell.id.clone();
        move || {
            focus.set_if_changed(Some(id.clone()));
        }
    };
    let checks = signal(Vec::new());
    let review = state.snapshots().clone();
    let snapshot_id = cell.id.clone();
    let statuses = memo(move || {
        let snapshot = review.status(&snapshot_id).map(Status::of_snapshot);
        checks.get().into_iter().chain(snapshot).collect::<Vec<_>>()
    });
    let (viewport, scale) = sizing(&entry, &cell);
    let seen = signal(false);
    let mounting = state.clone();
    let deferred = Lazy::new(
        LayoutStyle::column(),
        move || seen.get(),
        move || {
            Ok(vec![mounted(
                &mounting,
                &entry,
                &cell,
                &settings,
                scheme,
                Mount {
                    checks,
                    focused,
                    viewport,
                    scale,
                    plays,
                },
            )?])
        },
    )?;
    let slot = Container::new(
        LayoutStyle::column()
            .flex_shrink(0.0)
            .width(CELL_WIDTH)
            .height(viewport.height * scale),
        children![deferred],
    )?;
    let progress = scroll_progress(slot.layout_node(), ViewRange::Cover);
    effect(move || {
        if progress.get() > 0.0 {
            seen.set_if_changed(true);
        }
    });
    let card = StyledContainer::new(
        LayoutStyle::column()
            .flex_shrink(0.0)
            .width(CELL_OUTER_WIDTH)
            .gap(WORKBENCH_GRID / 2.0)
            .padding_all(PAD),
        move |_| {
            let tokens = use_workbench_tokens();
            let border = match focused.get() {
                true => Border::uniform(use_theme_tokens().primary(), FOCUSED_RING),
                false => Border::uniform(tokens.border_subtle, 1.0),
            };
            RectStyle::default()
                .with_fill(tokens.panel_background)
                .with_border(border)
                .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
        },
        vec![
            focus_toggle(label.clone(), focused, focus_this.clone())?,
            badges(statuses)?,
            details(statuses)?,
            box_item(slot),
        ],
    )?
    .role(Role::Group)
    .a11y_label(move || label.clone())
    .on_press(focus_this.clone())
    .on_alt_press(move |button| {
        if button == PointerButton::Secondary {
            focus_this();
            menu.open.set(Some(Asked {
                at: pointer_position().unwrap_or_default(),
                around: None,
            }));
        }
    });
    let column = Container::new(
        LayoutStyle::column().flex_shrink(0.0),
        vec![box_item(card), menu.layer],
    )?;
    Ok(box_item(column))
}

struct CellMenu {
    open: RwSignal<Option<Asked>>,
    layer: Box<dyn LayoutItem>,
}

/// The menu a secondary press on the cell opens: the preview on the Canvas view as the cell shows it.
fn cell_menu(
    state: &WorkshopState,
    entry: PreviewEntry,
    cell: MatrixCell,
) -> Result<CellMenu, LayoutError> {
    let open = signal(None::<Asked>);
    let state = state.clone();
    let rows = Children::one(move || {
        let (state, cell) = (state.clone(), cell.clone());
        item(
            ItemProps::props()
                .label(strings::reactive(OPEN_IN_CANVAS))
                .on_press(Rc::new(move || open_cell(&state, &entry, &cell)))
                .build(),
        )
    });
    let layer = menu_layer(open, rows)?;
    Ok(CellMenu { open, layer })
}

/// Names the cell, and focuses it when pressed: the keyboard's way to what a press anywhere on the cell does.
fn focus_toggle(
    label: String,
    focused: Memo<bool>,
    focus: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown = label.clone();
    let text = Text::declaring(
        move || shown.clone(),
        LayoutStyle::new().min_width(0.0),
        |text| workbench_mono(text).with_font_weight(600),
    )?;
    let toggle = StyledContainer::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .min_height(WORKBENCH_GRID * 3.0)
            .padding_horizontal(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default().with_radius(BorderRadius::all(WORKBENCH_RADIUS)),
        children![text],
    )?
    .control(Role::Button)
    .toggled(move || focused.get())
    .a11y_label(move || strings::text_with(FOCUS_CELL, &label))
    .on_press(focus);
    Ok(box_item(toggle))
}

fn badges(statuses: Memo<Vec<Status>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = ReactiveList::with_style(
        LayoutStyle::row().flex_wrap().gap(WORKBENCH_GRID / 2.0),
        move || statuses.get(),
        |status: &Status| format!("{status:?}"),
        |status| {
            let tone = status.tone();
            badge(
                BadgeProps::props()
                    .label(Reactive::of(move || status.text()))
                    .color(Reactive::of(move || tone_color(tone)))
                    .build(),
            )
        },
    )?;
    Ok(box_item(row))
}

/// Why a cell failed, under its badges: one line ending in an ellipsis at the cell's edge, so a failed cell stands as tall as its neighbours, with the whole of it in a tooltip and in the panels once the cell is focused.
fn details(statuses: Memo<Vec<Status>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let lines = ReactiveList::with_style(
        LayoutStyle::column(),
        move || {
            statuses.with(|statuses| {
                statuses
                    .iter()
                    .filter_map(|status| status.detail().map(str::to_string))
                    .collect::<Vec<_>>()
            })
        },
        |detail: &String| detail.clone(),
        |detail| {
            let line = detail.clone();
            tooltip(
                TooltipProps::props()
                    .text(Reactive::of(move || detail.clone()))
                    .children(Children::new(move || {
                        let line = line.clone();
                        let text = Text::declaring(
                            move || line.clone(),
                            LayoutStyle::new().max_width(CELL_WIDTH),
                            |text| workbench_muted(workbench_mono(text)).with_clamp(1, true),
                        )?;
                        Ok(vec![box_item(text)])
                    }))
                    .build(),
            )
        },
    )?;
    Ok(box_item(lines))
}

fn tone_color(tone: Tone) -> Color {
    let tokens = use_theme_tokens();
    match tone {
        Tone::Good => tokens.success(),
        Tone::Fair => tokens.warning(),
        Tone::Bad => tokens.error(),
        Tone::Quiet => tokens.muted(),
    }
}

struct Mount {
    checks: RwSignal<Vec<Status>>,
    focused: Memo<bool>,
    viewport: Size,
    scale: f32,
    /// Whether the cell runs the preview's play: see [`Plan::plays_in`](super::plan::Plan::plays_in).
    plays: bool,
}

/// The preview mounted for `cell` as `cargo telar test` mounts it, under the toolbar's settings, its play run and its accessibility checked on the canvas the play leaves, shown through a frame the inspection panels follow while the cell is focused.
fn mounted(
    state: &WorkshopState,
    entry: &PreviewEntry,
    cell: &MatrixCell,
    settings: &CanvasSettings,
    scheme: ColorScheme,
    mount: Mount,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Mount {
        checks,
        focused,
        viewport,
        scale,
        plays,
    } = mount;
    let failed = |message: String| {
        checks.update(|checks| checks.insert(0, Status::BuildFailed(message)));
        empty()
    };
    let ctx = match cell.ctx(entry) {
        Ok(ctx) => ctx,
        Err(error) => return failed(error.to_string()),
    };
    seed(&ctx.globals(), settings, scheme, entry, cell);
    let play = match Play::mount_with(entry, ctx, viewport) {
        Ok(play) => play,
        Err(error) => return failed(error.message),
    };
    let canvas = Rc::clone(play.canvas());
    canvas.set_scale(scale);
    let held: Rc<RefCell<Option<Play>>> = Rc::default();
    let keeper = Rc::clone(&held);
    run_when_sized(play, move |mut play| {
        checks.set(run_checks(&mut play, plays));
        held.replace(Some(play));
    });
    on_cleanup(move || drop(keeper.take()));
    let frame = SurfaceFrame::sized(Rc::clone(&canvas), LayoutStyle::new().flex_shrink(0.0))?;
    follow_while_focused(state, focused, canvas);
    let inspection = state.inspection();
    let marks = ReactiveList::with_style(
        LayoutStyle::new().absolute_fill(),
        move || match focused.get() {
            true => vec![()],
            false => Vec::new(),
        },
        |_: &()| (),
        move |()| inspection_marks(inspection),
    )?;
    let marks =
        Container::new(LayoutStyle::new().absolute_fill(), children![marks])?.input_transparent();
    let stack = Container::column(children![frame, marks])?;
    Ok(box_item(stack))
}

fn run_checks(play: &mut Play, plays: bool) -> Vec<Status> {
    let mut found = Vec::new();
    if play.entry().play.is_some() {
        found.push(match plays.then(|| guarded(|| play.run())) {
            None => Status::PlaySkipped,
            Some(outcome) => match outcome {
                Ok(Ok(())) => Status::PlayPassed,
                Ok(Err(error)) => Status::PlayFailed(error.message),
                Err(panic) => Status::PlayFailed(panic),
            },
        });
    }
    if let Ok(report) = guarded(|| play.check_a11y()) {
        let errors = report.at_least(Severity::Error).count();
        let all = report.violations.len();
        found.push(match (errors, all) {
            (0, 0) => Status::Accessible,
            (0, warnings) => Status::A11yWarnings(warnings),
            (errors, _) => Status::A11yErrors(errors),
        });
    }
    found
}

fn follow_while_focused(
    state: &WorkshopState,
    focused: Memo<bool>,
    canvas: Rc<telar::SurfaceCanvas>,
) {
    let inspection = state.inspection();
    let following: Rc<RefCell<Option<RelayoutHook>>> = Rc::default();
    let held = Rc::clone(&following);
    effect(move || {
        let hook = focused
            .get()
            .then(|| inspection.follow(PreviewOnly(Rc::clone(&canvas))));
        held.replace(hook);
    });
    on_cleanup(move || drop(following.take()));
}

fn empty() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}
