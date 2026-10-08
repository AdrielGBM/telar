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

/// One matrix: the values of each axis, by axis name. A key in [`MATRIX_GLOBAL_AXES`] varies the canvas's environment; any other key names an arg.
pub type MatrixAxes = BTreeMap<String, Vec<MatrixValue>>;

/// The axes a matrix varies the canvas's environment along, rather than an arg.
pub const MATRIX_GLOBAL_AXES: [&str; 5] = ["mode", "locale", "dir", "viewport", "control_size"];

/// The values `dir` takes.
const DIRECTIONS: [&str; 2] = ["ltr", "rtl"];

/// The values `control_size` takes.
const CONTROL_SIZES: [&str; 4] = ["mini", "small", "regular", "large"];

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
                .filter(|(name, _)| !is_name(name))
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
        let viewports = self.viewports.clone().unwrap_or_default();
        self.matrices
            .iter()
            .flatten()
            .flat_map(|(name, axes)| matrix_problems(name, axes, &viewports))
            .collect()
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn matrix_problems(
    name: &str,
    axes: &MatrixAxes,
    viewports: &BTreeMap<String, Viewport>,
) -> Vec<String> {
    let at = format!("`[telar.previews.matrices] {name}`");
    let mut problems = Vec::new();
    if !is_name(name) {
        problems.push(format!(
            "{at} is not a name: use letters, digits, `_` and `-`"
        ));
    }
    if axes.is_empty() {
        problems.push(format!(
            "{at} varies nothing: give it an axis, like `mode = [\"light\", \"dark\"]`"
        ));
    }
    for (axis, values) in axes {
        let at = format!("{at} axis `{axis}`");
        if values.is_empty() {
            problems.push(format!("{at} lists no values"));
        }
        let mut seen = Vec::new();
        for value in values {
            if seen.contains(&value) {
                problems.push(format!("{at} lists `{value}` more than once"));
            }
            seen.push(value);
        }
        if !MATRIX_GLOBAL_AXES.contains(&axis.as_str()) && !is_plain_identifier(axis) {
            problems.push(format!(
                "{at} is neither one of {} nor an arg name",
                MATRIX_GLOBAL_AXES.join(", ")
            ));
        }
        problems.extend(
            values
                .iter()
                .filter_map(|value| global_value_problem(axis, value, viewports))
                .map(|problem| format!("{at}: {problem}")),
        );
    }
    problems
}

/// Why `value` cannot be a value of the global axis `axis`. An arg axis takes any value its arg's type parses, which only the preview knows.
fn global_value_problem(
    axis: &str,
    value: &MatrixValue,
    viewports: &BTreeMap<String, Viewport>,
) -> Option<String> {
    let text = match value {
        MatrixValue::Text(text) => text.as_str(),
        _ if MATRIX_GLOBAL_AXES.contains(&axis) => {
            return Some(format!("`{value}` is not a string"));
        }
        _ => return None,
    };
    let allowed: &[&str] = match axis {
        "dir" => &DIRECTIONS,
        "control_size" => &CONTROL_SIZES,
        "viewport" => {
            return (Viewport::parse(text).is_none() && !viewports.contains_key(text)).then(|| {
                format!(
                    "\"{text}\" is neither a size like \"390x844\" nor a name in `[telar.previews.viewports]`"
                )
            });
        }
        "mode" | "locale" => {
            return text
                .trim()
                .is_empty()
                .then(|| "an empty string names nothing".to_string());
        }
        _ => return None,
    };
    (!allowed.contains(&text)).then(|| format!("\"{text}\" is not one of {}", allowed.join(", ")))
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
