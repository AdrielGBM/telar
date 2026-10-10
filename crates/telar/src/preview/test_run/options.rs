//! What a test run was asked to do, as `cargo telar test` passes it through the environment.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use telar_project::protocol::{
    TEST_A11Y_VAR, TEST_PACKAGE_DIR_VAR, TEST_PREVIEW_VAR, TEST_SNAPSHOTS_VAR, TEST_TARGET_DIR_VAR,
    TEST_TOLERANCE_CHANNEL_VAR, TEST_TOLERANCE_PIXELS_VAR,
};

/// What a run does with what [`a11y::check`](super::super::a11y::check) finds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum A11yLevel {
    /// Checks nothing.
    Off,
    /// Reports every finding and fails nothing.
    #[default]
    Warn,
    /// Fails a preview on any finding of [`Severity::Error`](super::super::a11y::Severity::Error), and reports the warnings.
    Error,
}

impl A11yLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        [Self::Off, Self::Warn, Self::Error]
            .into_iter()
            .find(|level| level.as_str() == word)
    }
}

/// What a run does with snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSetting {
    /// Takes none: what a run that was not told where the goldens live does.
    #[default]
    Off,
    /// Compares each preview and cell with its goldens, and fails on a drift or a missing golden.
    Compare,
    /// Writes the goldens that are missing or no longer match.
    Update,
}

impl SnapshotSetting {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Compare => "compare",
            Self::Update => "update",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        [Self::Off, Self::Compare, Self::Update]
            .into_iter()
            .find(|setting| setting.as_str() == word)
    }
}

/// Everything a test run reads from its environment. [`Default`] is a run nobody configured: every preview, accessibility findings reported, no snapshots and no report.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestOptions {
    /// Only the previews, or the matrix cells, whose id this matches.
    pub filter: Option<PreviewFilter>,
    pub a11y: A11yLevel,
    pub snapshots: SnapshotSetting,
    /// The package under test, whose `previews/__snapshots__` holds the goldens.
    pub package_dir: Option<PathBuf>,
    /// The Cargo target directory, under which the report and the diffs go.
    pub target_dir: Option<PathBuf>,
    /// The per-channel tolerance `[telar.previews.snapshots]` sets, over the default.
    pub channel_tolerance: Option<u8>,
    /// The differing pixels `[telar.previews.snapshots]` allows, over the default.
    pub max_differing_pixels: Option<u64>,
}

impl TestOptions {
    /// The options `cargo telar test` set on this process.
    pub fn from_env() -> Result<Self, OptionsError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// The options the variables `var` looks up describe; a blank one counts as unset.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, OptionsError> {
        let var = |name: &str| {
            var(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        let parsed = |name: &'static str| (name, var(name));
        Ok(Self {
            filter: var(TEST_PREVIEW_VAR).map(PreviewFilter::new),
            a11y: parse(parsed(TEST_A11Y_VAR), A11yLevel::parse)?.unwrap_or_default(),
            snapshots: parse(parsed(TEST_SNAPSHOTS_VAR), SnapshotSetting::parse)?
                .unwrap_or_default(),
            package_dir: var(TEST_PACKAGE_DIR_VAR).map(PathBuf::from),
            target_dir: var(TEST_TARGET_DIR_VAR).map(PathBuf::from),
            channel_tolerance: parse(parsed(TEST_TOLERANCE_CHANNEL_VAR), |word| word.parse().ok())?,
            max_differing_pixels: parse(parsed(TEST_TOLERANCE_PIXELS_VAR), |word| {
                word.parse().ok()
            })?,
        })
    }
}

fn parse<T>(
    (var, value): (&'static str, Option<String>),
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, OptionsError> {
    value
        .map(|value| parse(&value).ok_or(OptionsError { var, value }))
        .transpose()
}

/// A variable set to something a test run does not understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsError {
    pub var: &'static str,
    pub value: String,
}

impl fmt::Display for OptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} has the value {:?}, which a test run does not understand",
            self.var, self.value
        )
    }
}

impl std::error::Error for OptionsError {}

/// A glob over preview and cell ids: `*` stands for any run of characters, `?` for any one, and everything else for itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewFilter {
    glob: String,
}

impl PreviewFilter {
    pub fn new(glob: impl Into<String>) -> Self {
        Self { glob: glob.into() }
    }

    pub fn glob(&self) -> &str {
        &self.glob
    }

    pub fn matches(&self, id: &str) -> bool {
        glob_matches(
            &self.glob.chars().collect::<Vec<_>>(),
            &id.chars().collect::<Vec<_>>(),
        )
    }
}

/// Greedy matching that backtracks only to the last `*`, so a glob never costs more than the product of the two lengths.
fn glob_matches(glob: &[char], text: &[char]) -> bool {
    let (mut g, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match glob.get(g) {
            Some('*') => {
                star = Some((g, t));
                g += 1;
            }
            Some(c) if *c == '?' || *c == text[t] => {
                g += 1;
                t += 1;
            }
            _ => match star {
                Some((star_g, star_t)) => {
                    g = star_g + 1;
                    t = star_t + 1;
                    star = Some((star_g, star_t + 1));
                }
                None => return false,
            },
        }
    }
    glob[g..].iter().all(|c| *c == '*')
}

#[cfg(test)]
#[path = "options_test.rs"]
mod tests;
