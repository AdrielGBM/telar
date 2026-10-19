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

/// What the Matrix view varies: the selected preview's own matrix, or axes picked in its place, starting from the preview's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum MatrixAxes {
    #[default]
    Declared,
    Picked(Vec<PickedAxis>),
}

/// An axis picked for the matrix, and whether it takes only the values the preview's own matrix gives it rather than every value it has.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct PickedAxis {
    pub(crate) axis: CustomAxis,
    pub(crate) declared: bool,
}

impl PickedAxis {
    pub(crate) fn all_of(axis: CustomAxis) -> Self {
        Self {
            axis,
            declared: false,
        }
    }

    /// The axis as `declared`, the axes of the preview's own matrix, narrow it, or as it is with every value.
    pub(crate) fn axis_in(
        &self,
        declared: &[Axis],
        args: &[ArgState],
        matrices: &Matrices,
    ) -> Option<Axis> {
        let narrowed = declared
            .iter()
            .find(|axis| self.declared && CustomAxis::of(axis).as_ref() == Some(&self.axis));
        match narrowed {
            Some(axis) => Some(*axis),
            None => self.axis.axis(args, matrices),
        }
    }
}

/// The axes of `entry`'s own matrix, none where it has none or names one the project does not.
pub(crate) fn declared_axes(entry: &PreviewEntry, matrices: &Matrices) -> &'static [Axis] {
    entry
        .matrix
        .and_then(|matrix| matrix.axes(matrices).ok())
        .unwrap_or_default()
}

impl MatrixAxes {
    /// The axes the view varies for `entry`, each with whether the preview's own matrix narrows it.
    pub(crate) fn picked(&self, entry: &PreviewEntry, matrices: &Matrices) -> Vec<PickedAxis> {
        match self {
            Self::Declared => declared_axes(entry, matrices)
                .iter()
                .filter_map(CustomAxis::of)
                .map(|axis| PickedAxis {
                    axis,
                    declared: true,
                })
                .collect(),
            Self::Picked(picked) => picked.clone(),
        }
    }

    /// Whether the view shows `entry`'s own matrix.
    pub(crate) fn is_declared_for(&self, entry: &PreviewEntry) -> bool {
        matches!(self, Self::Declared) && entry.matrix.is_some()
    }

    /// With `axis` picked or not, every value of it taken once it is picked again, and back to the preview's own matrix once that is what is picked.
    pub(crate) fn toggled(
        &self,
        entry: &PreviewEntry,
        matrices: &Matrices,
        axis: &CustomAxis,
        on: bool,
    ) -> Self {
        let picked = toggled(&self.picked(entry, matrices), axis, on);
        let declared = Self::Declared.picked(entry, matrices);
        let same =
            picked.len() == declared.len() && picked.iter().all(|axis| declared.contains(axis));
        match same {
            true => Self::Declared,
            false => Self::Picked(picked),
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
    /// The axis the view picks for one a preview's own matrix varies; `None` for a kind of axis this workshop does not know.
    pub(crate) fn of(axis: &Axis) -> Option<Self> {
        Some(match axis {
            Axis::Mode(_) | Axis::RegisteredModes => Self::Mode,
            Axis::Locale(_) => Self::Locale,
            Axis::Dir(_) => Self::Direction,
            Axis::Viewport(_) | Axis::ProjectViewports => Self::Viewport,
            Axis::ControlSize(_) => Self::ControlSize,
            Axis::Arg(name, _) => Self::Arg(name.to_string()),
            _ => return None,
        })
    }

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

pub(crate) fn toggled(axes: &[PickedAxis], axis: &CustomAxis, picked: bool) -> Vec<PickedAxis> {
    let mut axes: Vec<PickedAxis> = axes
        .iter()
        .filter(|held| held.axis != *axis)
        .cloned()
        .collect();
    if picked {
        axes.push(PickedAxis::all_of(axis.clone()));
        axes.sort_by_key(|picked| picked.axis.rank());
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
    /// Whether the cells are the preview's own matrix rather than one built from the axes picked.
    pub(crate) own: bool,
}

impl Plan {
    pub(crate) fn capped(&self) -> bool {
        self.total > self.cells.len()
    }

    /// Whether `cell` runs the preview's play: every cell of the preview's own matrix does, since its author declared the play holds across it, and a cell of one built from the axes picked only where its args are the preview's defaults, which are what the play was written against.
    pub(crate) fn plays_in(&self, cell: &MatrixCell, args: &[ArgState]) -> bool {
        self.own
            || cell.args.iter().all(|(name, value)| {
                args.iter()
                    .any(|arg| arg.name == *name && arg.default.as_ref() == Some(value))
            })
    }
}

/// The cells of `entry`'s matrix as `axes` asks for it.
pub(crate) fn plan(
    entry: &PreviewEntry,
    axes: &MatrixAxes,
    args: &[ArgState],
    matrices: &Matrices,
) -> Result<Plan, Problem> {
    let own = axes.is_declared_for(entry);
    let expanded = match (axes, entry.matrix) {
        (MatrixAxes::Declared, Some(matrix)) => matrix.cells(entry, matrices),
        (MatrixAxes::Declared, None) => return Err(Problem::NoMatrix),
        (MatrixAxes::Picked(picked), _) => {
            let declared = declared_axes(entry, matrices);
            let axes: Vec<Axis> = picked
                .iter()
                .filter_map(|axis| axis.axis_in(declared, args, matrices))
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
    Ok(Plan { cells, total, own })
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
