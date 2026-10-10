//! What the Matrix view shows: the cells of the selected preview's own matrix or of one built from the axes picked, as many as it mounts, laid out with the last axis across and the others down.

use serde::{Deserialize, Serialize};
use telar::preview::host::ArgState;
use telar::preview::{
    Axis, ControlKind, MATRIX_CELL_CAP, Matrices, Matrix, MatrixCell, PreviewEntry,
};
use telar::{ControlSize, Direction};

use crate::settings::PRESET_VIEWPORTS;
use crate::strings::{CONTROL_SIZE, DIRECTION, LOCALE, MODE, VIEWPORT};

/// Past this many cells the view says a matrix takes a while to mount, short of the cap that stops it.
pub(crate) const MATRIX_WARN_AT: usize = 48;

const DIRECTIONS: &[Direction] = &[Direction::Ltr, Direction::Rtl];
const CONTROL_SIZES: &[ControlSize] = &[
    ControlSize::Mini,
    ControlSize::Small,
    ControlSize::Regular,
    ControlSize::Large,
];
const TOGGLE_VALUES: &[&str] = &["false", "true"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum MatrixSource {
    #[default]
    Preview,
    Custom,
}

impl MatrixSource {
    /// What `entry`'s matrix view shows when this is asked for: a preview with no matrix of its own shows the built one.
    pub(crate) fn shown_for(self, entry: &PreviewEntry) -> Self {
        match entry.matrix {
            Some(_) => self,
            None => Self::Custom,
        }
    }
}

/// An axis the view can build a matrix from: one of the environment's globals, or an arg with a fixed set of values.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum CustomAxis {
    Mode,
    Locale,
    Direction,
    ControlSize,
    Viewport,
    Arg(String),
}

/// The globals a built matrix varies, in the order its axes nest, each with its [`crate::strings`] key.
pub(crate) const GLOBAL_AXES: [(CustomAxis, &str); 5] = [
    (CustomAxis::Mode, MODE),
    (CustomAxis::Locale, LOCALE),
    (CustomAxis::Direction, DIRECTION),
    (CustomAxis::ControlSize, CONTROL_SIZE),
    (CustomAxis::Viewport, VIEWPORT),
];

impl CustomAxis {
    /// `None` for an arg the preview does not have, or one with no fixed set of values. The locales are the application catalog's, the viewports the project's or, where it names none, the workshop's presets.
    pub(crate) fn axis(&self, args: &[ArgState], matrices: &Matrices) -> Option<Axis> {
        Some(match self {
            Self::Mode => Axis::RegisteredModes,
            Self::Locale => Axis::Locale(telar::i18n::catalog().map_or(&[], |c| c.locales)),
            Self::Direction => Axis::Dir(DIRECTIONS),
            Self::ControlSize => Axis::ControlSize(CONTROL_SIZES),
            Self::Viewport => match matrices.viewports().is_empty() {
                true => Axis::Viewport(PRESET_VIEWPORTS),
                false => Axis::ProjectViewports,
            },
            Self::Arg(name) => {
                let arg = args.iter().find(|arg| arg.name == name)?;
                Axis::Arg(arg.name, arg_values(&arg.control)?)
            }
        })
    }

    /// Where this goes among `axes` picked so far, so a built matrix nests its axes the same way whatever order they were picked in: the globals first, then the args in the order they were picked.
    fn rank(&self) -> usize {
        GLOBAL_AXES
            .iter()
            .position(|(global, _)| global == self)
            .unwrap_or(GLOBAL_AXES.len())
    }
}

pub(crate) fn toggled(axes: &[CustomAxis], axis: &CustomAxis, picked: bool) -> Vec<CustomAxis> {
    let mut axes: Vec<CustomAxis> = axes.iter().filter(|held| *held != axis).cloned().collect();
    if picked {
        axes.push(axis.clone());
        axes.sort_by_key(CustomAxis::rank);
    }
    axes
}

/// The args of `args` a built matrix can vary: those whose control picks among a fixed set of values.
pub(crate) fn varied_args(args: &[ArgState]) -> Vec<&'static str> {
    args.iter()
        .filter(|arg| arg_values(&arg.control).is_some())
        .map(|arg| arg.name)
        .collect()
}

fn arg_values(control: &ControlKind) -> Option<&'static [&'static str]> {
    match control {
        ControlKind::Toggle => Some(TOGGLE_VALUES),
        ControlKind::Choice { variants } if !variants.is_empty() => Some(variants),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    /// The preview declares no matrix and no axis is picked.
    NoMatrix,
    /// The built matrix is asked for and no axis is picked.
    NoAxes,
    Invalid(String),
    /// The matrix expands to no cells, as one varying only the registered modes does before any is registered.
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    /// At most [`MATRIX_CELL_CAP`], the first axis outermost.
    pub(crate) cells: Vec<MatrixCell>,
    pub(crate) total: usize,
}

impl Plan {
    pub(crate) fn capped(&self) -> bool {
        self.total > self.cells.len()
    }
}

/// The cells of `entry`'s matrix as `source` asks for it, built from `axes` where that is the built one.
pub(crate) fn plan(
    entry: &PreviewEntry,
    source: MatrixSource,
    axes: &[CustomAxis],
    args: &[ArgState],
    matrices: &Matrices,
) -> Result<Plan, Problem> {
    let expanded = match (source.shown_for(entry), entry.matrix) {
        (MatrixSource::Preview, Some(matrix)) => matrix.cells(entry, matrices),
        _ => {
            let axes: Vec<Axis> = axes
                .iter()
                .filter_map(|axis| axis.axis(args, matrices))
                .collect();
            if axes.is_empty() {
                return Err(match entry.matrix {
                    Some(_) => Problem::NoAxes,
                    None => Problem::NoMatrix,
                });
            }
            Matrix::expand(&axes, entry, matrices)
        }
    };
    let mut cells = expanded.map_err(|error| Problem::Invalid(error.to_string()))?;
    if cells.is_empty() {
        return Err(Problem::Empty);
    }
    let total = cells.len();
    cells.truncate(MATRIX_CELL_CAP);
    Ok(Plan { cells, total })
}

/// The cells laid out as the view shows them: a column per value of the last axis, and a row per combination of the others.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Grid {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<GridRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridRow {
    /// What heads the row, `axis: value · …`; `None` for a matrix of one axis, whose one row needs no heading.
    pub(crate) heading: Option<String>,
    /// In column order; a row the cap cut short has fewer cells than there are columns.
    pub(crate) cells: Vec<MatrixCell>,
}

pub(crate) fn grid(cells: &[MatrixCell]) -> Grid {
    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<GridRow> = Vec::new();
    for cell in cells {
        let Some((last, outer)) = cell.coordinates.split_last() else {
            continue;
        };
        let column = coordinate(last);
        if !columns.contains(&column) {
            columns.push(column);
        }
        let heading = (!outer.is_empty()).then(|| label(outer));
        match rows.last_mut() {
            Some(row) if row.heading == heading => row.cells.push(cell.clone()),
            _ => rows.push(GridRow {
                heading,
                cells: vec![cell.clone()],
            }),
        }
    }
    Grid { columns, rows }
}

/// What a cell is called: each of its coordinates, `axis: value`, joined by ` · `.
pub(crate) fn cell_label(cell: &MatrixCell) -> String {
    label(&cell.coordinates)
}

fn label(coordinates: &[(&'static str, String)]) -> String {
    coordinates
        .iter()
        .map(coordinate)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn coordinate((axis, value): &(&'static str, String)) -> String {
    format!("{axis}: {value}")
}
