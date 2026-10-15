//! The Matrix view: the selected preview once per cell of its matrix, or of one built from the axes picked, side by side under headings for the axes.
//!
//! Each cell mounts the first time it scrolls into view, on a canvas of its own in the cell's environment over the toolbar's, and runs the preview's play and accessibility checks there; its badges say how they came out, with the snapshot status the last `cargo telar test` recorded. Pressing a cell focuses it: the Layout and Accessibility panels and the measure and outline marks follow its canvas, the Snapshot panel reviews its snapshot, and "Open in canvas" opens the preview on the Canvas view in the cell's globals and args.

mod cell;
mod plan;

use std::rc::Rc;

use telar::preview::{Matrices, Matrix, MatrixCell, PreviewEntry, control_size_word};
use telar::{
    Accessible, AlignItems, Children, Color, ColorScheme, Container, JustifyContent, LayoutError,
    LayoutItem, LayoutScrollArea, LayoutStyle, Reactive, ReactiveList, RectStyle, Role, RwSignal,
    ShapeStyle, StyledContainer, Text, batch, box_item, children, effect, signal,
    use_resolved_scheme, use_theme_tokens,
};
use telar_components::{
    ButtonProps, ButtonVariant, CheckboxProps, Tab, button, checkbox, tab_list,
};
use telar_devtools::{WORKBENCH_GRID, workbench_fill, workbench_muted};

pub(crate) use plan::{CustomAxis, MatrixSource};

use self::cell::CELL_OUTER_WIDTH;
use self::plan::{GLOBAL_AXES, Grid, MATRIX_WARN_AT, Plan, Problem, grid, toggled, varied_args};
use crate::canvas::{HEADER_HEIGHT, states};
use crate::settings::{CanvasSettings, CanvasSize, ModeChoice};
use crate::state::{ViewMode, WorkshopState};
use crate::strings::{
    self, MATRIX_AXES, MATRIX_CAPPED, MATRIX_CELLS, MATRIX_CUSTOM, MATRIX_EMPTY, MATRIX_INVALID,
    MATRIX_LARGE, MATRIX_NAMED, MATRIX_NONE, MATRIX_OWN, MATRIX_PICK, MATRIX_SOURCE,
    OPEN_IN_CANVAS,
};

const ROW_HEADING_WIDTH: f32 = WORKBENCH_GRID * 20.0;
const GAP: f32 = WORKBENCH_GRID * 2.0;

pub(crate) fn matrix(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if state.entries().is_empty() {
        return states::empty();
    }
    let selected = state.clone();
    let building = state.clone();
    let page = ReactiveList::with_style(
        workbench_fill(),
        move || selected.selected().into_iter().collect(),
        |entry: &PreviewEntry| entry.id,
        move |entry| page(&building, entry),
    )?;
    Ok(box_item(page))
}

fn page(state: &WorkshopState, entry: PreviewEntry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shown: RwSignal<Rc<[MatrixCell]>> = signal(Rc::from([]));
    let column = Container::new(
        workbench_fill(),
        vec![header(state, entry, shown)?, cells(state, entry, shown)?],
    )?;
    Ok(box_item(column))
}

fn header(
    state: &WorkshopState,
    entry: PreviewEntry,
    shown: RwSignal<Rc<[MatrixCell]>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = format!("{} / {}", entry.title, entry.name);
    let title = Text::declaring(
        move || title.clone(),
        LayoutStyle::new().min_width(0.0),
        |text| text.with_font_weight(600),
    )?;
    let mut items = children![title];
    if let Some(matrix) = entry.matrix {
        items.push(source_switch(state, matrix)?);
    }
    items.push(axis_picker(state, entry)?);
    items.push(box_item(Container::new(
        LayoutStyle::new().flex_grow(1.0),
        Vec::new(),
    )?));
    items.push(open_in_canvas(state, entry, shown)?);
    let header = StyledContainer::new(
        LayoutStyle::row()
            .flex_wrap()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .min_height(HEADER_HEIGHT)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID / 2.0)
            .gap(WORKBENCH_GRID * 1.5),
        |_| {
            let tokens = telar_devtools::use_workbench_tokens();
            RectStyle::default().with_fill(tokens.panel_background)
        },
        items,
    )?;
    Ok(box_item(header))
}

fn source_switch(
    state: &WorkshopState,
    matrix: Matrix,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let own = Reactive::of(move || match matrix {
        Matrix::Named(name) => strings::text_with(MATRIX_NAMED, name),
        _ => strings::text(MATRIX_OWN),
    });
    tab_list(
        state.matrix_source(),
        vec![
            Tab::new(MatrixSource::Preview, own),
            Tab::new(MatrixSource::Custom, strings::reactive(MATRIX_CUSTOM)),
        ],
        Color::TRANSPARENT.into(),
        Some(strings::reactive(MATRIX_SOURCE)),
    )
}

/// The axes a built matrix can vary, each picked or not, shown while the built matrix is.
fn axis_picker(
    state: &WorkshopState,
    entry: PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = state.matrix_source();
    let building = state.clone();
    let picker = ReactiveList::with_style(
        LayoutStyle::row(),
        move || match source.get().shown_for(&entry) {
            MatrixSource::Custom => vec![()],
            MatrixSource::Preview => Vec::new(),
        },
        |_: &()| (),
        move |()| axes(&building, &entry),
    )?;
    Ok(box_item(picker))
}

fn axes(state: &WorkshopState, entry: &PreviewEntry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let args = state.args(entry).states();
    let matrices = Matrices::installed();
    let varies = |axis: &CustomAxis| {
        axis.axis(&args, &matrices)
            .is_some_and(|axis| Matrix::count(&[axis], &matrices) > 1)
    };
    let mut boxes = Vec::new();
    for (axis, key) in GLOBAL_AXES.into_iter().filter(|(axis, _)| varies(axis)) {
        boxes.push(axis_box(state, axis, strings::reactive(key))?);
    }
    for name in varied_args(&args) {
        boxes.push(axis_box(
            state,
            CustomAxis::Arg(name.to_string()),
            Reactive::of(move || name.to_string()),
        )?);
    }
    let group = Container::new(
        LayoutStyle::row()
            .flex_wrap()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 1.5),
        boxes,
    )?
    .role(Role::Group)
    .a11y_label(|| strings::text(MATRIX_AXES));
    Ok(box_item(group))
}

fn axis_box(
    state: &WorkshopState,
    axis: CustomAxis,
    label: Reactive<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picked = state.matrix_axes();
    let checked = signal(false);
    let watched = axis.clone();
    effect(move || {
        let on = picked.with(|axes| axes.contains(&watched));
        checked.set_if_changed(on);
    });
    let on_toggle: Rc<dyn Fn(bool)> =
        Rc::new(move |on| picked.update(|axes| *axes = toggled(axes, &axis, on)));
    checkbox(
        CheckboxProps::props()
            .checked(checked)
            .label(label)
            .on_toggle(on_toggle)
            .build(),
        Children::default(),
    )
}

/// Opens the preview on the Canvas view as the focused cell shows it, offered while a cell shown is focused.
fn open_in_canvas(
    state: &WorkshopState,
    entry: PreviewEntry,
    shown: RwSignal<Rc<[MatrixCell]>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let focus = state.matrix_focus();
    let opening = state.clone();
    let offered = ReactiveList::with_style(
        LayoutStyle::row(),
        move || {
            let focused = focus.get();
            shown.with(|cells| {
                cells
                    .iter()
                    .find(|cell| Some(&cell.id) == focused.as_ref())
                    .cloned()
                    .into_iter()
                    .collect()
            })
        },
        |cell: &MatrixCell| cell.id.clone(),
        move |cell| {
            let state = opening.clone();
            button(
                ButtonProps::props()
                    .label(strings::reactive(OPEN_IN_CANVAS))
                    .variant(ButtonVariant::Ghost)
                    .on_press(Rc::new(move || open_cell(&state, &entry, &cell)))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(offered))
}

/// Shows `entry` on the Canvas view with the toolbar set to `cell`'s globals and its args to the cell's.
fn open_cell(state: &WorkshopState, entry: &PreviewEntry, cell: &MatrixCell) {
    batch(|| {
        let globals = &cell.globals;
        state.canvas_settings().update(|settings| {
            if let Some(mode) = &globals.mode {
                settings.mode = ModeChoice::for_mode(mode);
            }
            if let Some(locale) = &globals.locale {
                settings.locale = Some(locale.clone());
            }
            if let Some(direction) = globals.direction {
                settings.direction = Some(direction);
            }
            if let Some(size) = globals.control_size {
                settings.control_size = Some(size);
            }
            if let Some(viewport) = globals.viewport {
                settings.size = CanvasSize::Custom {
                    width: viewport.width,
                    height: viewport.height,
                };
                settings.rotated = false;
            }
        });
        let args = state.args(entry);
        for (name, value) in &cell.args {
            let _ = args.set(name, value.clone());
        }
        state.view().set(ViewMode::Canvas);
    });
}

/// What the cells are built from: a change to any of it mounts every cell afresh, so each runs its play again in what it now shows.
#[derive(Clone)]
struct Asked {
    source: MatrixSource,
    axes: Vec<CustomAxis>,
    settings: Rc<CanvasSettings>,
    scheme: ColorScheme,
    remounts: u64,
}

impl Asked {
    /// Keyed on the mode the cells resolve for a preview asking for `asked`, so a change of scheme remounts them only where it changes what they show.
    fn key(&self, asked: Option<&str>) -> impl std::hash::Hash + use<> {
        let settings = &self.settings;
        (
            self.source,
            self.axes.clone(),
            settings.mode.mode(asked, self.scheme),
            settings.locale.clone(),
            settings.direction,
            settings.control_size.map(control_size_word),
            settings.high_contrast,
            settings.background,
            self.remounts,
        )
    }
}

fn cells(
    state: &WorkshopState,
    entry: PreviewEntry,
    shown: RwSignal<Rc<[MatrixCell]>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let asking = state.clone();
    let building = state.clone();
    let cells = ReactiveList::with_style(
        workbench_fill(),
        move || {
            vec![Asked {
                source: asking.matrix_source().get(),
                axes: asking.matrix_axes().get(),
                settings: Rc::new(asking.canvas_settings().get()),
                scheme: use_resolved_scheme(),
                remounts: asking.remounts(),
            }]
        },
        move |asked: &Asked| asked.key(entry.env.mode),
        move |asked| {
            let args = building.args(&entry).states();
            let planned = plan::plan(
                &entry,
                asked.source,
                &asked.axes,
                &args,
                &Matrices::installed(),
            );
            match planned {
                Ok(plan) => {
                    shown.set(Rc::from(plan.cells.as_slice()));
                    keep_focus_on(&building, &plan.cells);
                    planned_cells(&building, entry, &plan, asked.settings, asked.scheme)
                }
                Err(problem) => {
                    shown.set(Rc::from([]));
                    notice(problem)
                }
            }
        },
    )?;
    Ok(box_item(cells))
}

/// Focuses the first of `cells` unless one of them already is, so the inspection panels always have a cell to follow.
fn keep_focus_on(state: &WorkshopState, cells: &[MatrixCell]) {
    let focus = state.matrix_focus();
    let held = focus.peek_with(|focused| {
        focused
            .as_deref()
            .is_some_and(|id| cells.iter().any(|cell| cell.id == id))
    });
    if !held {
        focus.set(cells.first().map(|cell| cell.id.clone()));
    }
}

fn planned_cells(
    state: &WorkshopState,
    entry: PreviewEntry,
    plan: &Plan,
    settings: Rc<CanvasSettings>,
    scheme: ColorScheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = Vec::new();
    if let Some(warning) = warning(plan) {
        items.push(warning);
    }
    items.push(table(state, entry, &grid(&plan.cells), &settings, scheme)?);
    let column = Container::new(
        LayoutStyle::column()
            .align_items(AlignItems::FLEX_START)
            .gap(GAP)
            .padding_all(GAP),
        items,
    )?;
    let scroll = LayoutScrollArea::new(workbench_fill(), box_item(column))?;
    Ok(box_item(scroll))
}

fn warning(plan: &Plan) -> Option<Box<dyn LayoutItem>> {
    let (shown, total) = (plan.cells.len(), plan.total);
    let text: Box<dyn Fn() -> String> = if plan.capped() {
        Box::new(move || {
            strings::text_with_values(
                MATRIX_CAPPED,
                &[("shown", &shown.to_string()), ("total", &total.to_string())],
            )
        })
    } else if total > MATRIX_WARN_AT {
        Box::new(move || strings::text_with(MATRIX_LARGE, &total.to_string()))
    } else {
        return None;
    };
    let text = Text::declaring(text, LayoutStyle::new(), |text| {
        text.with_color(use_theme_tokens().warning())
    })
    .ok()?;
    let status = Container::new(LayoutStyle::row(), children![text])
        .ok()?
        .role(Role::Status);
    Some(box_item(status))
}

/// The column headings over a row of cells for each combination of the outer axes, each row under its heading.
fn table(
    state: &WorkshopState,
    entry: PreviewEntry,
    grid: &Grid,
    settings: &Rc<CanvasSettings>,
    scheme: ColorScheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let headed = grid.rows.iter().any(|row| row.heading.is_some());
    let mut lines = Vec::new();
    let mut headings = Vec::new();
    if headed {
        headings.push(heading(None, ROW_HEADING_WIDTH)?);
    }
    for column in &grid.columns {
        headings.push(heading(Some(column.clone()), CELL_OUTER_WIDTH)?);
    }
    lines.push(line(headings)?);
    for row in &grid.rows {
        let mut items = Vec::new();
        if headed {
            items.push(heading(row.heading.clone(), ROW_HEADING_WIDTH)?);
        }
        for matrix_cell in &row.cells {
            items.push(cell::cell(
                state,
                entry,
                matrix_cell.clone(),
                Rc::clone(settings),
                scheme,
            )?);
        }
        lines.push(line(items)?);
    }
    let table = Container::new(LayoutStyle::column().gap(GAP), lines)?
        .role(Role::Group)
        .a11y_label(|| strings::text(MATRIX_CELLS));
    Ok(box_item(table))
}

fn line(items: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::FLEX_START)
            .gap(GAP),
        items,
    )?;
    Ok(box_item(line))
}

fn heading(text: Option<String>, width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = Vec::new();
    if let Some(text) = text {
        items.push(box_item(Text::declaring(
            move || text.clone(),
            LayoutStyle::new().min_width(0.0),
            |text| workbench_muted(text.with_font_weight(600)),
        )?));
    }
    let heading = Container::new(
        LayoutStyle::column()
            .flex_shrink(0.0)
            .width(width)
            .padding_horizontal(WORKBENCH_GRID),
        items,
    )?;
    Ok(box_item(heading))
}

fn notice(problem: Problem) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || match &problem {
            Problem::NoMatrix => strings::text(MATRIX_NONE),
            Problem::NoAxes => strings::text(MATRIX_PICK),
            Problem::Invalid(error) => strings::text_with(MATRIX_INVALID, error),
            Problem::Empty => strings::text(MATRIX_EMPTY),
        },
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = Container::new(
        workbench_fill()
            .padding_all(GAP)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        children![text],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

#[cfg(test)]
#[path = "matrix_test.rs"]
mod tests;
