//! Matrices: one preview rendered across several environments at once.
//!
//! A matrix is a list of [`Axis`] values, each varying one global of the canvas or one arg, and the preview is rendered once per combination of their values: a [`MatrixCell`]. A preview writes one inline with [`Matrix::Axes`], or names one with [`Matrix::Named`]: one of the package's `[telar.previews.matrices]`, which `app!` installs as the binary loads, or the built-in [`NamedMatrix::THEMES`].

use std::fmt;
use std::sync::{PoisonError, RwLock};

use crate::{ControlSize, Direction, Size};

use super::host::{ArgError, Args};
use super::{ArgValue, Globals, PreviewCtx, PreviewEntry};

/// How many cells a matrix view mounts before it stops and says how many it left out. Expanding a matrix is never capped: snapshots and the test runner take every cell.
pub const MATRIX_CELL_CAP: usize = 64;

/// The cells a preview is rendered across.
///
/// `const` all the way down, so a generated table and a Rust preview both write one in place: `.matrix(Matrix::Axes(&[Axis::Mode(&["light", "dark"]), Axis::Arg("size", &["12", "16"])]))`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matrix {
    /// A matrix the package names under `[telar.previews.matrices]`, or a built-in one such as `themes`.
    Named(&'static str),
    /// The axes themselves, the first outermost.
    Axes(&'static [Axis]),
}

/// One thing a matrix varies, and the values it takes.
///
/// Every value is a literal, so an inline matrix stays a promotable constant inside a fn body.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Modes registered with `register_mode`, by id.
    Mode(&'static [&'static str]),
    /// Every mode registered when the matrix is expanded, as [`Matrices::installed`] reads them or [`Matrices::with_modes`] lists them.
    RegisteredModes,
    /// BCP 47 language tags.
    Locale(&'static [&'static str]),
    Dir(&'static [Direction]),
    /// Canvas sizes, each written `"390x844"` or named in `[telar.previews] viewports`.
    Viewport(&'static [&'static str]),
    ControlSize(&'static [ControlSize]),
    /// An arg, by name, and its values in [`ArgValue`]'s text form: `"12"`, `"true"`, `"Primary"`, `"\"Save\""`.
    Arg(&'static str, &'static [&'static str]),
}

impl Axis {
    /// The name a cell id and a grid header give the axis: the global's key in `[telar.previews.matrices]`, or the arg's name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Mode(_) | Self::RegisteredModes => "mode",
            Self::Locale(_) => "locale",
            Self::Dir(_) => "dir",
            Self::Viewport(_) => "viewport",
            Self::ControlSize(_) => "control_size",
            Self::Arg(name, _) => name,
        }
    }

    fn len(&self, matrices: &Matrices) -> usize {
        match self {
            Self::Mode(values) | Self::Locale(values) | Self::Viewport(values) => values.len(),
            Self::RegisteredModes => matrices.modes.len(),
            Self::Dir(values) => values.len(),
            Self::ControlSize(values) => values.len(),
            Self::Arg(_, values) => values.len(),
        }
    }

    fn settings(&self, matrices: &Matrices) -> Result<Vec<Setting>, MatrixError> {
        let axis = self.name();
        Ok(match *self {
            Self::Mode(modes) => modes
                .iter()
                .map(|mode| Setting::Mode(mode.to_string()))
                .collect(),
            Self::RegisteredModes => matrices.modes.iter().cloned().map(Setting::Mode).collect(),
            Self::Locale(locales) => locales
                .iter()
                .map(|locale| Setting::Locale(locale.to_string()))
                .collect(),
            Self::Dir(directions) => directions.iter().copied().map(Setting::Dir).collect(),
            Self::Viewport(viewports) => viewports
                .iter()
                .map(|text| {
                    matrices
                        .viewport(text)
                        .map(|size| Setting::Viewport(text, size))
                        .ok_or(MatrixError::Viewport { text })
                })
                .collect::<Result<_, _>>()?,
            Self::ControlSize(sizes) => sizes.iter().copied().map(Setting::ControlSize).collect(),
            Self::Arg(name, texts) => texts
                .iter()
                .map(|text| {
                    text.parse()
                        .map(|value| Setting::Arg(name, value))
                        .map_err(|_| MatrixError::ArgValue { axis, text })
                })
                .collect::<Result<_, _>>()?,
        })
    }
}

/// One value of an axis, as a cell applies it.
#[derive(Clone, Debug)]
enum Setting {
    Mode(String),
    Locale(String),
    Dir(Direction),
    Viewport(&'static str, Size),
    ControlSize(ControlSize),
    Arg(&'static str, ArgValue),
}

impl Setting {
    fn label(&self) -> String {
        match self {
            Self::Mode(text) | Self::Locale(text) => text.clone(),
            Self::Dir(direction) => direction_word(*direction).to_string(),
            Self::Viewport(text, _) => text.trim().to_string(),
            Self::ControlSize(size) => control_size_word(*size).to_string(),
            Self::Arg(_, value) => value.to_string(),
        }
    }

    fn apply(&self, cell: &mut MatrixCell) {
        let globals = &mut cell.globals;
        match self {
            Self::Mode(mode) => globals.mode = Some(mode.clone()),
            Self::Locale(locale) => globals.locale = Some(locale.clone()),
            Self::Dir(direction) => globals.direction = Some(*direction),
            Self::Viewport(_, size) => globals.viewport = Some(*size),
            Self::ControlSize(size) => globals.control_size = Some(*size),
            Self::Arg(name, value) => cell.args.push((*name, value.clone())),
        }
    }
}

const DIRECTION_WORDS: [(&str, Direction); 2] = [("ltr", Direction::Ltr), ("rtl", Direction::Rtl)];

const CONTROL_SIZE_WORDS: [(&str, ControlSize); 4] = [
    ("mini", ControlSize::Mini),
    ("small", ControlSize::Small),
    ("regular", ControlSize::Regular),
    ("large", ControlSize::Large),
];

fn word_of<T: Copy + PartialEq>(words: &[(&'static str, T)], value: T) -> &'static str {
    words
        .iter()
        .find(|(_, held)| *held == value)
        .map_or("", |(word, _)| word)
}

fn value_of<T: Copy>(words: &[(&str, T)], word: &str) -> Option<T> {
    words
        .iter()
        .find(|(held, _)| *held == word)
        .map(|(_, value)| *value)
}

/// The word a matrix's `dir` axis and a workshop link write `direction` as: `ltr` or `rtl`.
pub fn direction_word(direction: Direction) -> &'static str {
    word_of(&DIRECTION_WORDS, direction)
}

/// The direction [`direction_word`] writes as `word`.
pub fn parse_direction(word: &str) -> Option<Direction> {
    value_of(&DIRECTION_WORDS, word)
}

/// The word a matrix's `control_size` axis and a workshop link write `size` as: `mini`, `small`, `regular` or `large`.
pub fn control_size_word(size: ControlSize) -> &'static str {
    word_of(&CONTROL_SIZE_WORDS, size)
}

/// The control size [`control_size_word`] writes as `word`.
pub fn parse_control_size(word: &str) -> Option<ControlSize> {
    value_of(&CONTROL_SIZE_WORDS, word)
}

/// A matrix a preview names rather than writes out.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedMatrix {
    pub name: &'static str,
    pub axes: &'static [Axis],
}

impl NamedMatrix {
    /// Every registered mode, side by side: the matrix a widget's look is checked across. A package's own `themes` takes its place.
    pub const THEMES: Self = Self::new("themes", &[Axis::RegisteredModes]);

    pub const fn new(name: &'static str, axes: &'static [Axis]) -> Self {
        Self { name, axes }
    }
}

/// A canvas size named in `[telar.previews] viewports`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportPreset {
    pub name: &'static str,
    /// In logical px.
    pub size: Size,
}

impl ViewportPreset {
    pub const fn new(name: &'static str, width: f32, height: f32) -> Self {
        Self {
            name,
            size: Size::new(width, height),
        }
    }
}

/// What a matrix is expanded against: the matrices and viewports the package names, and the modes registered for [`Axis::RegisteredModes`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Matrices {
    named: &'static [NamedMatrix],
    viewports: &'static [ViewportPreset],
    modes: Vec<String>,
}

#[derive(Clone, Copy)]
struct Installed {
    named: &'static [NamedMatrix],
    viewports: &'static [ViewportPreset],
}

// A process-wide static rather than a thread local: a constructor installs it before any thread exists, and a hot-reload dylib carries its own copy, installed as it loads.
static INSTALLED: RwLock<Option<Installed>> = RwLock::new(None);

impl Matrices {
    pub fn new(named: &'static [NamedMatrix], viewports: &'static [ViewportPreset]) -> Self {
        Self {
            named,
            viewports,
            modes: Vec::new(),
        }
    }

    /// The matrices and viewports the application's `telar.toml` names, as `app!` installed them, or none, with the modes registered now for [`Axis::RegisteredModes`], which [`Self::with_modes`] overrides.
    pub fn installed() -> Self {
        let installed = *INSTALLED.read().unwrap_or_else(PoisonError::into_inner);
        let matrices = installed.map_or_else(Self::default, |installed| {
            Self::new(installed.named, installed.viewports)
        });
        matrices.with_modes(theme_core::registered_modes())
    }

    /// The modes [`Axis::RegisteredModes`] takes, in order.
    pub fn with_modes<M: Into<String>>(self, modes: impl IntoIterator<Item = M>) -> Self {
        Self {
            modes: modes.into_iter().map(Into::into).collect(),
            ..self
        }
    }

    /// The matrix `name` names: the package's, or else a built-in one.
    pub fn get(&self, name: &str) -> Option<&'static [Axis]> {
        self.named
            .iter()
            .chain([&NamedMatrix::THEMES])
            .find(|matrix| matrix.name == name)
            .map(|matrix| matrix.axes)
    }

    /// The size a viewport axis value stands for: `"390x844"`, or a name in `[telar.previews] viewports`.
    pub fn viewport(&self, text: &str) -> Option<Size> {
        let text = text.trim();
        self.viewports
            .iter()
            .find(|preset| preset.name == text)
            .map(|preset| preset.size)
            .or_else(|| parse_size(text))
    }

    pub fn named(&self) -> &'static [NamedMatrix] {
        self.named
    }

    pub fn viewports(&self) -> &'static [ViewportPreset] {
        self.viewports
    }

    pub fn modes(&self) -> &[String] {
        &self.modes
    }
}

/// A size written `WIDTHxHEIGHT`, each a finite number of logical px above zero, as a viewport is in a matrix and in a link.
pub fn parse_size(text: &str) -> Option<Size> {
    let (width, height) = text.split_once(['x', 'X'])?;
    let side = |text: &str| {
        text.trim()
            .parse::<f32>()
            .ok()
            .filter(|side| side.is_finite() && *side > 0.0)
    };
    Some(Size::new(side(width)?, side(height)?))
}

/// Installs the matrices and viewports an application's `telar.toml` names, from the constructor `app!` emits.
#[doc(hidden)]
pub fn __install_matrices(named: &'static [NamedMatrix], viewports: &'static [ViewportPreset]) {
    *INSTALLED.write().unwrap_or_else(PoisonError::into_inner) =
        Some(Installed { named, viewports });
}

/// [`__install_matrices`], unless an application installed its own: what `rsx_modules!` emits, so the one `app!` installs wins whichever constructor runs first.
#[doc(hidden)]
pub fn __install_matrices_if_unset(
    named: &'static [NamedMatrix],
    viewports: &'static [ViewportPreset],
) {
    INSTALLED
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert(Installed { named, viewports });
}

impl Matrix {
    /// The axes this matrix varies.
    pub fn axes(&self, matrices: &Matrices) -> Result<&'static [Axis], MatrixError> {
        match *self {
            Self::Axes(axes) => Ok(axes),
            Self::Named(name) => matrices.get(name).ok_or(MatrixError::Unknown { name }),
        }
    }

    /// How many cells [`Self::cells`] would expand to, without expanding them: what a view checks against [`MATRIX_CELL_CAP`]. Saturates rather than overflowing.
    pub fn cell_count(&self, matrices: &Matrices) -> Result<usize, MatrixError> {
        Ok(self
            .axes(matrices)?
            .iter()
            .map(|axis| axis.len(matrices))
            .filter(|len| *len > 0)
            .reduce(usize::saturating_mul)
            .unwrap_or(0))
    }

    /// One cell per combination of the axes' values, the first axis outermost, each with the id `<preview-id>--<axis>=<value>…` that deep links and snapshot files name it by.
    ///
    /// An axis with no values, such as [`Axis::RegisteredModes`] before any mode is registered, varies nothing and is left out; a matrix left with no axis has no cells.
    pub fn cells(
        &self,
        entry: &PreviewEntry,
        matrices: &Matrices,
    ) -> Result<Vec<MatrixCell>, MatrixError> {
        let mut axes: Vec<(&'static str, Vec<AxisValue>)> = Vec::new();
        for axis in self.axes(matrices)? {
            let name = axis.name();
            if axes.iter().any(|(seen, _)| *seen == name) {
                return Err(MatrixError::RepeatedAxis { axis: name });
            }
            let mut values: Vec<AxisValue> = Vec::new();
            for setting in axis.settings(matrices)? {
                let label = setting.label();
                let segment = id_segment(&label);
                if segment.is_empty() {
                    return Err(MatrixError::Unnamed {
                        axis: name,
                        value: label,
                    });
                }
                if values.iter().any(|value| value.segment == segment) {
                    return Err(MatrixError::RepeatedValue {
                        axis: name,
                        value: label,
                    });
                }
                values.push(AxisValue {
                    segment,
                    label,
                    setting,
                });
            }
            axes.push((name, values));
        }
        axes.retain(|(_, values)| !values.is_empty());
        if axes.is_empty() {
            return Ok(Vec::new());
        }
        let mut cells = vec![MatrixCell::of(entry)];
        for (axis, values) in &axes {
            cells = cells
                .iter()
                .flat_map(|cell| values.iter().map(|value| value.extend(cell, axis)))
                .collect();
        }
        Ok(cells)
    }
}

/// One value of an axis, with what a cell id and a grid header call it.
struct AxisValue {
    segment: String,
    label: String,
    setting: Setting,
}

impl AxisValue {
    fn extend(&self, cell: &MatrixCell, axis: &'static str) -> MatrixCell {
        let mut cell = cell.clone();
        cell.id = format!("{}--{axis}={}", cell.id, self.segment);
        cell.coordinates.push((axis, self.label.clone()));
        self.setting.apply(&mut cell);
        cell
    }
}

/// `text` as a cell id carries it: letters, digits, `.`, `_`, `-` and `+` kept as written, and every run of anything else, quotes and spaces included, made one `-` between them.
fn id_segment(text: &str) -> String {
    let mut segment = String::with_capacity(text.len());
    let mut gap = false;
    for c in text.chars() {
        if c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+') {
            if gap && !segment.is_empty() && !segment.ends_with('-') && c != '-' {
                segment.push('-');
            }
            gap = false;
            segment.push(c);
        } else {
            gap = true;
        }
    }
    segment
}

/// One combination of a matrix's values: the preview rendered with these globals and args.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixCell {
    /// `<preview-id>--<axis>=<value>…`, in axis order.
    pub id: String,
    /// Each axis's name and this cell's value on it, in axis order: what a grid's headers show.
    pub coordinates: Vec<(&'static str, String)>,
    /// The globals the cell sets over the preview's own [`PreviewEnv`](super::PreviewEnv).
    pub globals: CellGlobals,
    /// The args the cell sets, in axis order.
    pub args: Vec<(&'static str, ArgValue)>,
}

/// The globals a [`MatrixCell`] sets. `None` leaves the preview's own setting, or the host's, in place.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellGlobals {
    /// A mode registered with `register_mode`, by id.
    pub mode: Option<String>,
    /// A BCP 47 language tag.
    pub locale: Option<String>,
    pub direction: Option<Direction>,
    /// The canvas size, in logical px.
    pub viewport: Option<Size>,
    pub control_size: Option<ControlSize>,
}

impl MatrixCell {
    fn of(entry: &PreviewEntry) -> Self {
        Self {
            id: entry.id.to_string(),
            coordinates: Vec::new(),
            globals: CellGlobals::default(),
            args: Vec::new(),
        }
    }

    /// Sets each global the cell names, and leaves the ones it does not.
    pub fn seed(&self, globals: &Globals) {
        let CellGlobals {
            mode,
            locale,
            direction,
            viewport,
            control_size,
        } = &self.globals;
        if let Some(mode) = mode {
            globals.mode().set(Some(mode.clone()));
        }
        if let Some(locale) = locale {
            globals.locale().set(Some(locale.clone()));
        }
        if let Some(direction) = direction {
            globals.direction().set(Some(*direction));
        }
        if let Some(viewport) = viewport {
            globals.viewport().set(Some(*viewport));
        }
        if let Some(control_size) = control_size {
            globals.control_size().set(Some(*control_size));
        }
    }

    /// The canvas this cell mounts `entry` in: args held in memory only, linked to its props and with its declared args listed, then set to the cell's; globals seeded from its [`PreviewEnv`](super::PreviewEnv), then from the cell.
    pub fn ctx(&self, entry: &PreviewEntry) -> Result<PreviewCtx, ArgError> {
        let args = Args::new();
        if let Some(props) = entry.props {
            args.link_props(props());
        }
        args.declare(entry.args);
        for (name, value) in &self.args {
            args.set(name, value.clone())?;
        }
        let globals = Globals::seeded(&entry.env);
        self.seed(&globals);
        Ok(PreviewCtx::from(args).with_globals(globals))
    }
}

/// Why a matrix has no cells to give.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatrixError {
    /// [`Matrix::Named`] names a matrix neither the package nor Telar has.
    Unknown { name: &'static str },
    /// Two axes vary the same thing.
    RepeatedAxis { axis: &'static str },
    /// One axis lists a value twice, or two values that name the same cell.
    RepeatedValue { axis: &'static str, value: String },
    /// A value with no letter or digit to name its cell by.
    Unnamed { axis: &'static str, value: String },
    /// A viewport that is neither a size nor a name in `[telar.previews] viewports`.
    Viewport { text: &'static str },
    /// An arg value that is not in [`ArgValue`]'s text form.
    ArgValue {
        axis: &'static str,
        text: &'static str,
    },
}

impl fmt::Display for MatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { name } => write!(
                f,
                "no matrix is named `{name}`: name one in `[telar.previews.matrices]`"
            ),
            Self::RepeatedAxis { axis } => write!(f, "the matrix varies `{axis}` twice"),
            Self::RepeatedValue { axis, value } => {
                write!(f, "matrix axis `{axis}` lists `{value}` more than once")
            }
            Self::Unnamed { axis, value } => write!(
                f,
                "matrix axis `{axis}` value `{value}` needs a letter or digit to name its cell"
            ),
            Self::Viewport { text } => write!(
                f,
                "matrix viewport \"{text}\" is neither a size like \"390x844\" nor a name in `[telar.previews] viewports`"
            ),
            Self::ArgValue { axis, text } => {
                write!(f, "matrix axis `{axis}` value `{text}` is not an arg value")
            }
        }
    }
}

impl std::error::Error for MatrixError {}

#[cfg(test)]
#[path = "matrix_test.rs"]
mod tests;
