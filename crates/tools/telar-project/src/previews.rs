//! `[telar.previews]`: the crates whose previews an application lists after its own, and the viewports and matrices its previews name.
//!
//! `include` is read by `telar::app!`, which lists each crate's `telar_all_previews()` after the package's own. A crate it names has to be one the package depends on, which [`previews_include_problems`] checks against `Cargo.toml` so the error lands on the key rather than on the generated call.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::declared::{DeclaredEntry, EntryProblem, PackageDependencies, declarations};
use crate::prelude::is_plain_identifier;

/// The `[telar.previews]` table.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PreviewsSection {
    /// The crates whose `telar_all_previews()` the application lists after its own, in this order.
    pub include: Option<Vec<PreviewsInclude>>,
    /// Named viewport sizes, which a matrix's `viewport` axis and a host's viewport presets offer by name.
    pub viewports: Option<BTreeMap<String, Viewport>>,
    /// Named matrices, which a preview asks for by name.
    pub matrices: Option<BTreeMap<String, MatrixAxes>>,
}

/// One matrix as `telar.toml` writes it: the values of each axis, by axis name. A key in [`MATRIX_GLOBAL_AXES`] varies the canvas's environment; any other key names an arg. Read through [`PreviewsSection::named_matrices`].
pub type MatrixAxes = BTreeMap<String, Vec<MatrixValue>>;

/// The axes a matrix varies the canvas's environment along, rather than an arg, in the order a matrix lists them.
pub const MATRIX_GLOBAL_AXES: [&str; 5] = ["mode", "locale", "dir", "viewport", "control_size"];

/// One axis of a named matrix, read and checked.
#[derive(Debug, Clone, PartialEq)]
pub enum MatrixAxis {
    /// Mode ids.
    Mode(Vec<String>),
    /// BCP 47 language tags.
    Locale(Vec<String>),
    Dir(Vec<MatrixDirection>),
    /// Each a size like `"390x844"` or a name in `[telar.previews.viewports]`.
    Viewport(Vec<String>),
    ControlSize(Vec<MatrixControlSize>),
    /// An arg, by name, and each value in the text form a preview's arg values are written in: a bool, a number, `#ff8800`, a choice such as `Primary`, or text in quotes, `'"Save"'`.
    Arg {
        name: String,
        values: Vec<String>,
    },
}

impl MatrixAxis {
    /// The key the axis is written under.
    pub fn name(&self) -> &str {
        match self {
            Self::Mode(_) => "mode",
            Self::Locale(_) => "locale",
            Self::Dir(_) => "dir",
            Self::Viewport(_) => "viewport",
            Self::ControlSize(_) => "control_size",
            Self::Arg { name, .. } => name,
        }
    }
}

/// A value of a matrix's `dir` axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixDirection {
    Ltr,
    Rtl,
}

impl MatrixDirection {
    const WORDS: [(&str, Self); 2] = [("ltr", Self::Ltr), ("rtl", Self::Rtl)];

    /// The direction `word` names: `ltr` or `rtl`.
    pub fn parse(word: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(held, _)| *held == word)
            .map(|(_, direction)| *direction)
    }
}

/// A value of a matrix's `control_size` axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixControlSize {
    Mini,
    Small,
    Regular,
    Large,
}

impl MatrixControlSize {
    const WORDS: [(&str, Self); 4] = [
        ("mini", Self::Mini),
        ("small", Self::Small),
        ("regular", Self::Regular),
        ("large", Self::Large),
    ];

    /// The control size `word` names: `mini`, `small`, `regular` or `large`.
    pub fn parse(word: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(held, _)| *held == word)
            .map(|(_, size)| *size)
    }
}

/// One value of a matrix axis, as `telar.toml` writes it.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum MatrixValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl std::fmt::Display for MatrixValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
        }
    }
}

/// One `[telar.previews] include` entry: a crate, spelled as Rust names it.
///
/// Only constructed through [`PreviewsInclude::parse`], so an entry that exists can be emitted as `::<crate>::telar_all_previews()`.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String")]
pub struct PreviewsInclude(String);

impl PreviewsInclude {
    /// Reads one entry as written in `telar.toml`, where it may be spelled as `Cargo.toml` names the package (`telar-components`): it is normalised to the crate name Rust sees (`telar_components`).
    pub fn parse(declared: &str) -> Result<Self, String> {
        let name = declared.trim().replace('-', "_");
        if !is_plain_identifier(&name) {
            return Err(format!(
                "`[telar.previews] include` entry \"{declared}\" is not a crate name. Name a crate whose previews to list, like \"telar-components\""
            ));
        }
        Ok(Self(name))
    }

    pub fn crate_name(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PreviewsInclude {
    type Error = String;

    fn try_from(declared: String) -> Result<Self, Self::Error> {
        Self::parse(&declared)
    }
}

impl std::fmt::Display for PreviewsInclude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A canvas size in logical px, written `"390x844"`.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    /// Reads `WIDTHxHEIGHT`, each a whole number of logical px above zero.
    pub fn parse(text: &str) -> Option<Self> {
        let (width, height) = text.trim().split_once(['x', 'X'])?;
        let width: u32 = width.trim().parse().ok()?;
        let height: u32 = height.trim().parse().ok()?;
        (width > 0 && height > 0).then_some(Self { width, height })
    }
}

impl TryFrom<String> for Viewport {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| {
            format!("viewport \"{text}\" is not a size: write it WIDTHxHEIGHT, like \"390x844\"")
        })
    }
}

impl PreviewsSection {
    /// The crates to list previews from, in order.
    pub fn include(&self) -> &[PreviewsInclude] {
        self.include.as_deref().unwrap_or_default()
    }

    /// This table's own keys, falling back to `base` key by key.
    pub(crate) fn over(self, base: Self) -> Self {
        Self {
            include: self.include.or(base.include),
            viewports: self.viewports.or(base.viewports),
            matrices: self.matrices.or(base.matrices),
        }
    }

    /// What a single file's table gets wrong on its own.
    pub(crate) fn file_problems(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut problems: Vec<String> = self
            .include()
            .iter()
            .filter(|entry| !seen.insert(entry.crate_name()))
            .map(|entry| format!("`[telar.previews] include` lists `{entry}` more than once"))
            .collect();
        problems.extend(
            self.viewports
                .iter()
                .flatten()
                .filter(|(name, _)| !is_preview_name(name))
                .map(|(name, _)| {
                    format!(
                        "`[telar.previews.viewports]` name \"{name}\" is not a name: use letters, digits, `_` and `-`"
                    )
                }),
        );
        problems
    }

    /// What the table, inherited keys included, gets wrong: each matrix against the axes it may vary and the viewports it may name.
    pub(crate) fn problems(&self) -> Vec<String> {
        self.named_matrices().err().unwrap_or_default()
    }

    /// Every named matrix, by name, with its axes read and checked, or every problem found in them. A manifest [`crate::TelarManifest::load`] returned has none.
    ///
    /// The global axes come first, in [`MATRIX_GLOBAL_AXES`] order, then the args by name: a TOML table keeps no order serde can see, and an order fixed here keeps a cell's id the same however the keys are written.
    pub fn named_matrices(&self) -> Result<BTreeMap<String, Vec<MatrixAxis>>, Vec<String>> {
        let viewports = self.viewports.clone().unwrap_or_default();
        let mut matrices = BTreeMap::new();
        let mut problems = Vec::new();
        for (name, axes) in self.matrices.iter().flatten() {
            match parse_matrix(name, axes, &viewports) {
                Ok(axes) => {
                    matrices.insert(name.clone(), axes);
                }
                Err(found) => problems.extend(found),
            }
        }
        if problems.is_empty() {
            Ok(matrices)
        } else {
            Err(problems)
        }
    }
}

/// Whether `name` is a name as `[telar.previews]` writes one: letters, digits, `_` and `-`.
pub fn is_preview_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Where `axis` falls in a matrix: the global axes in [`MATRIX_GLOBAL_AXES`] order, then the args.
fn axis_rank(axis: &str) -> usize {
    MATRIX_GLOBAL_AXES
        .iter()
        .position(|global| *global == axis)
        .unwrap_or(MATRIX_GLOBAL_AXES.len())
}

fn parse_matrix(
    name: &str,
    axes: &MatrixAxes,
    viewports: &BTreeMap<String, Viewport>,
) -> Result<Vec<MatrixAxis>, Vec<String>> {
    let at = format!("`[telar.previews.matrices] {name}`");
    let mut problems = Vec::new();
    if !is_preview_name(name) {
        problems.push(format!(
            "{at} is not a name: use letters, digits, `_` and `-`"
        ));
    }
    if axes.is_empty() {
        problems.push(format!(
            "{at} varies nothing: give it an axis, like `mode = [\"light\", \"dark\"]`"
        ));
    }
    let mut ordered: Vec<_> = axes.iter().collect();
    ordered.sort_by_key(|(axis, _)| axis_rank(axis));
    let mut parsed = Vec::new();
    for (axis, values) in ordered {
        match parse_axis(&format!("{at} axis `{axis}`"), axis, values, viewports) {
            Ok(axis) => parsed.push(axis),
            Err(found) => problems.extend(found),
        }
    }
    if problems.is_empty() {
        Ok(parsed)
    } else {
        Err(problems)
    }
}

fn parse_axis(
    at: &str,
    axis: &str,
    values: &[MatrixValue],
    viewports: &BTreeMap<String, Viewport>,
) -> Result<MatrixAxis, Vec<String>> {
    let mut problems = Vec::new();
    if values.is_empty() {
        problems.push(format!("{at} lists no values"));
    }
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            problems.push(format!("{at} lists `{value}` more than once"));
        }
    }
    let problems_of = &mut problems;
    let parsed = match axis {
        "mode" => MatrixAxis::Mode(read_each(at, values, problems_of, named_text)),
        "locale" => MatrixAxis::Locale(read_each(at, values, problems_of, named_text)),
        "dir" => MatrixAxis::Dir(read_each(at, values, problems_of, |value| {
            word(value, &MatrixDirection::WORDS)
        })),
        "viewport" => MatrixAxis::Viewport(read_each(at, values, problems_of, |value| {
            viewport_text(value, viewports)
        })),
        "control_size" => MatrixAxis::ControlSize(read_each(at, values, problems_of, |value| {
            word(value, &MatrixControlSize::WORDS)
        })),
        _ if is_plain_identifier(axis) => MatrixAxis::Arg {
            name: axis.to_string(),
            values: values.iter().map(arg_text).collect(),
        },
        _ => {
            problems.push(format!(
                "{at} is neither one of {} nor an arg name",
                MATRIX_GLOBAL_AXES.join(", ")
            ));
            return Err(problems);
        }
    };
    if problems.is_empty() {
        Ok(parsed)
    } else {
        Err(problems)
    }
}

fn read_each<T>(
    at: &str,
    values: &[MatrixValue],
    problems: &mut Vec<String>,
    read: impl Fn(&MatrixValue) -> Result<T, String>,
) -> Vec<T> {
    values
        .iter()
        .filter_map(|value| {
            read(value)
                .map_err(|problem| problems.push(format!("{at}: {problem}")))
                .ok()
        })
        .collect()
}

fn text(value: &MatrixValue) -> Result<&str, String> {
    match value {
        MatrixValue::Text(text) => Ok(text.trim()),
        _ => Err(format!("`{value}` is not a string")),
    }
}

fn named_text(value: &MatrixValue) -> Result<String, String> {
    match text(value)? {
        "" => Err("an empty string names nothing".to_string()),
        text => Ok(text.to_string()),
    }
}

fn viewport_text(
    value: &MatrixValue,
    viewports: &BTreeMap<String, Viewport>,
) -> Result<String, String> {
    let text = text(value)?;
    if Viewport::parse(text).is_some() || viewports.contains_key(text) {
        Ok(text.to_string())
    } else {
        Err(format!(
            "\"{text}\" is neither a size like \"390x844\" nor a name in `[telar.previews.viewports]`"
        ))
    }
}

fn word<T: Copy>(value: &MatrixValue, words: &[(&str, T)]) -> Result<T, String> {
    let text = text(value)?;
    words
        .iter()
        .find(|(word, _)| *word == text)
        .map(|(_, value)| *value)
        .ok_or_else(|| {
            let allowed: Vec<&str> = words.iter().map(|(word, _)| *word).collect();
            format!("\"{text}\" is not one of {}", allowed.join(", "))
        })
}

/// `value` in the text form a preview's arg values are written in. A string is taken as already in it, since only the arg's type can tell a choice from text.
fn arg_text(value: &MatrixValue) -> String {
    match value {
        MatrixValue::Float(value) if value.is_nan() => "+NaN".to_string(),
        MatrixValue::Float(value) if value.is_infinite() => {
            if *value > 0.0 { "+inf" } else { "-inf" }.to_string()
        }
        MatrixValue::Float(value) => format!("{value:?}"),
        MatrixValue::Text(text) => text.clone(),
        value => value.to_string(),
    }
}

/// One `[telar.previews] include` entry, and the `telar.toml` line that declares it.
pub type PreviewsIncludeDeclaration = DeclaredEntry<PreviewsInclude>;

/// A `[telar.previews] include` entry `app!` would call and the package cannot reach, said about the line that declares it.
pub type PreviewsIncludeProblem = EntryProblem<PreviewsInclude>;

/// The `include` list [`crate::TelarManifest::load`] reads for `package_dir`, each entry paired with where it is written.
pub fn previews_include_declarations(
    package_dir: &Path,
) -> Result<Vec<PreviewsIncludeDeclaration>, crate::ManifestError> {
    declarations(
        package_dir,
        |telar| telar.previews.and_then(|previews| previews.include),
        PreviewsInclude::parse,
    )
}

/// Every `[telar.previews] include` entry of `package_dir` that names a crate its `Cargo.toml` does not list under `[dependencies]`, or the package itself.
///
/// Said against the key rather than left to rustc, whose answer is an unresolved path inside the code `app!` expands to. A `Cargo.toml` that cannot be read answers nothing rather than flagging every entry.
pub fn previews_include_problems(
    package_dir: &Path,
) -> Result<Vec<PreviewsIncludeProblem>, crate::ManifestError> {
    let declarations = previews_include_declarations(package_dir)?;
    let Some(dependencies) = PackageDependencies::read(package_dir) else {
        return Ok(Vec::new());
    };
    let own = dependencies.crate_name();
    Ok(declarations
        .into_iter()
        .filter_map(|declaration| {
            let name = declaration.entry.crate_name().to_string();
            let (message, help) = if name == own {
                (
                    format!(
                        "`[telar.previews] include` entry `{name}` is `{}` itself",
                        dependencies.package
                    ),
                    "its own previews are listed already: remove the entry".to_string(),
                )
            } else if !dependencies.reaches(&name) {
                (
                    format!(
                        "`[telar.previews] include` entry `{name}` is not a dependency of `{}`",
                        dependencies.package
                    ),
                    dependencies.missing_help(&name, "`app!`"),
                )
            } else {
                return None;
            };
            Some(PreviewsIncludeProblem {
                declaration,
                message,
                help,
            })
        })
        .collect())
}

#[cfg(test)]
#[path = "previews_test.rs"]
mod tests;
