//! What a bake found wrong, as data: what went wrong, the file it is with and, where the bake knows it, the reference in a package's sources it came from. Each caller words and places it where its user is looking, a terminal line for the CLI, a diagnostic on a buffer's line for the analyzer.

use std::fmt;
use std::path::PathBuf;

use i18n_core::SourceProblem;
use telar_project::{AssetContext, AssetKind, ManifestError};

/// Whether a build may go on from a problem. An error leaves everything else baked, so an editor keeps answering for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug)]
pub struct BakeProblem {
    pub kind: ProblemKind,
    /// What the problem is with: the `telar.toml`, the asset or source file, the `.telar/` directory, or the package or dependency directory whose record is at fault.
    pub file: PathBuf,
}

/// A `src:"…"` reference to an asset file, its path as the bake keys it, with `/` separators.
#[derive(Debug, Clone)]
pub struct AssetRef {
    pub kind: &'static AssetKind,
    pub path: String,
}

/// Where in a package's sources a problem came from.
#[derive(Debug, Clone, Copy)]
pub enum Origin<'a> {
    /// Every `src:"…"` naming this asset path, wherever in the package it is written.
    Asset(&'a AssetRef),
    /// A 1-based line of [`BakeProblem::file`].
    Line(usize),
}

#[derive(Debug)]
pub enum ProblemKind {
    Manifest(ManifestError),
    AssetMissing {
        asset: AssetRef,
        error: String,
    },
    AssetUndecodable {
        asset: AssetRef,
        reason: String,
    },
    IdNotLiteral {
        line: usize,
        kind: &'static AssetKind,
        tag: String,
        prop: String,
        written: String,
    },
    IconIdInvalid {
        line: usize,
        reason: String,
    },
    IconNotFound {
        line: usize,
        id: String,
        searched: String,
    },
    IconUnresolvable {
        line: usize,
        id: String,
        reason: String,
    },
    IconUndecodable {
        line: usize,
        id: String,
        reason: String,
    },
    IconSetUnlisted {
        line: usize,
        set: String,
        message: String,
    },
    /// Its icons are left unbaked, unlike a dependency's refused set, which already shipped.
    IconSetRefused {
        line: usize,
        set: String,
        message: String,
    },
    DependencyIconSetUnlisted {
        dependency: String,
        set: String,
        message: String,
    },
    /// Still ships, because the dependency already baked it.
    DependencyIconSetRefused {
        dependency: String,
        set: String,
        message: String,
    },
    /// Its icons are left out of the package's licence notice.
    DependencyIconsUnread {
        dependency: String,
        library: bool,
        problem: String,
    },
    IconRecordUnreadable {
        problem: String,
    },
    IconRecordUnwritable {
        error: String,
    },
    ArtifactUngenerated {
        reason: String,
    },
    ArtifactUnwritable {
        error: String,
    },
    CatalogUnreadable {
        error: String,
    },
    CatalogInvalid {
        problem: SourceProblem,
    },
    /// The file is still baked, but a change to it will not mark the catalog stale.
    CatalogNameNotUtf8,
}

/// Every problem one bake turned up, in the order it found them.
#[derive(Debug, Default)]
pub struct Problems(Vec<BakeProblem>);

impl Problems {
    pub(crate) fn push(&mut self, problem: BakeProblem) {
        self.0.push(problem);
    }

    pub fn as_slice(&self) -> &[BakeProblem] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, BakeProblem> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn warnings(&self) -> impl Iterator<Item = &BakeProblem> {
        self.with_severity(Severity::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &BakeProblem> {
        self.with_severity(Severity::Error)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    fn with_severity(&self, severity: Severity) -> impl Iterator<Item = &BakeProblem> {
        self.0
            .iter()
            .filter(move |problem| problem.severity() == severity)
    }
}

impl Extend<BakeProblem> for Problems {
    fn extend<I: IntoIterator<Item = BakeProblem>>(&mut self, problems: I) {
        self.0.extend(problems);
    }
}

impl IntoIterator for Problems {
    type Item = BakeProblem;
    type IntoIter = std::vec::IntoIter<BakeProblem>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Problems {
    type Item = &'a BakeProblem;
    type IntoIter = std::slice::Iter<'a, BakeProblem>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl BakeProblem {
    pub fn new(kind: ProblemKind, file: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            file: file.into(),
        }
    }

    pub fn severity(&self) -> Severity {
        match self.kind {
            ProblemKind::IconSetUnlisted { .. }
            | ProblemKind::DependencyIconSetUnlisted { .. }
            | ProblemKind::DependencyIconsUnread { .. }
            | ProblemKind::IconRecordUnreadable { .. }
            | ProblemKind::IconRecordUnwritable { .. }
            | ProblemKind::CatalogNameNotUtf8 => Severity::Warning,
            ProblemKind::Manifest(_)
            | ProblemKind::AssetMissing { .. }
            | ProblemKind::AssetUndecodable { .. }
            | ProblemKind::IdNotLiteral { .. }
            | ProblemKind::IconIdInvalid { .. }
            | ProblemKind::IconNotFound { .. }
            | ProblemKind::IconUnresolvable { .. }
            | ProblemKind::IconUndecodable { .. }
            | ProblemKind::IconSetRefused { .. }
            | ProblemKind::DependencyIconSetRefused { .. }
            | ProblemKind::ArtifactUngenerated { .. }
            | ProblemKind::ArtifactUnwritable { .. }
            | ProblemKind::CatalogUnreadable { .. }
            | ProblemKind::CatalogInvalid { .. } => Severity::Error,
        }
    }

    /// `None` where the bake knows no line or `src:"…"` it came from: the package's `telar.toml`, a dependency's icons, its artifacts or records, or a catalog file as a whole.
    pub fn origin(&self) -> Option<Origin<'_>> {
        match &self.kind {
            ProblemKind::AssetMissing { asset, .. }
            | ProblemKind::AssetUndecodable { asset, .. } => Some(Origin::Asset(asset)),
            ProblemKind::IdNotLiteral { line, .. }
            | ProblemKind::IconIdInvalid { line, .. }
            | ProblemKind::IconNotFound { line, .. }
            | ProblemKind::IconUnresolvable { line, .. }
            | ProblemKind::IconUndecodable { line, .. }
            | ProblemKind::IconSetUnlisted { line, .. }
            | ProblemKind::IconSetRefused { line, .. } => Some(Origin::Line(*line)),
            ProblemKind::CatalogInvalid { problem } => problem.line().map(Origin::Line),
            ProblemKind::Manifest(_)
            | ProblemKind::DependencyIconSetUnlisted { .. }
            | ProblemKind::DependencyIconSetRefused { .. }
            | ProblemKind::DependencyIconsUnread { .. }
            | ProblemKind::IconRecordUnreadable { .. }
            | ProblemKind::IconRecordUnwritable { .. }
            | ProblemKind::ArtifactUngenerated { .. }
            | ProblemKind::ArtifactUnwritable { .. }
            | ProblemKind::CatalogUnreadable { .. }
            | ProblemKind::CatalogNameNotUtf8 => None,
        }
    }
}

/// The problem without its [`Origin::Line`], which each caller states in its own terms.
impl fmt::Display for BakeProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.display();
        match &self.kind {
            ProblemKind::Manifest(error) => write!(f, "{error}"),
            ProblemKind::AssetMissing { asset, error } => write!(
                f,
                "cannot bake {} asset `{}`: not found at {file} ({error})",
                asset.kind.label, asset.path
            ),
            ProblemKind::AssetUndecodable { asset, reason } => write!(
                f,
                "cannot bake {} asset `{}` from {file}: {reason}",
                asset.kind.label, asset.path
            ),
            ProblemKind::IdNotLiteral {
                kind,
                tag,
                prop,
                written,
                ..
            } => {
                let message = AssetContext::dynamic_id_message(kind, tag, prop);
                write!(
                    f,
                    "`{prop}:{written}` — {}",
                    message.trim_start_matches("rsx: ")
                )
            }
            ProblemKind::IconIdInvalid { reason, .. } => write!(f, "{reason}"),
            ProblemKind::IconNotFound { id, searched, .. } => {
                write!(f, "no icon source has `{id}`: it is in none of {searched}")
            }
            ProblemKind::IconUnresolvable { id, reason, .. } => {
                write!(f, "could not resolve `{id}`: {reason}")
            }
            ProblemKind::IconUndecodable { id, reason, .. } => {
                write!(f, "cannot bake icon `{id}`: {reason}")
            }
            ProblemKind::IconSetUnlisted { message, .. }
            | ProblemKind::IconSetRefused { message, .. } => write!(f, "{message}"),
            ProblemKind::DependencyIconSetUnlisted {
                dependency,
                message,
                ..
            }
            | ProblemKind::DependencyIconSetRefused {
                dependency,
                message,
                ..
            } => write!(
                f,
                "`{dependency}`, which this package is built with, ships icons this package's licence policy has not accepted: {message}"
            ),
            ProblemKind::DependencyIconsUnread {
                dependency,
                library,
                problem,
            } => write!(
                f,
                "the icons `{dependency}` draws are left out of this package's licence notice: in {file}, {problem}. {}",
                if *library {
                    "A library has to be packaged with a `cargo telar` that ships its icon record"
                } else {
                    "Bake it again to rewrite its record"
                }
            ),
            ProblemKind::IconRecordUnreadable { problem } => write!(
                f,
                "in {file}, {problem}; this bake replaces it with only what it resolves itself"
            ),
            ProblemKind::IconRecordUnwritable { error } => {
                write!(f, "could not write the icon record in {file}: {error}")
            }
            ProblemKind::ArtifactUngenerated { reason } => {
                write!(f, "could not bake assets for {file}: {reason}")
            }
            ProblemKind::ArtifactUnwritable { error } => {
                write!(f, "could not write {file}: {error}")
            }
            ProblemKind::CatalogUnreadable { error } => {
                write!(f, "catalog: reading {file}: {error}")
            }
            ProblemKind::CatalogInvalid { problem } => match problem {
                _ if problem.line().is_some() => write!(f, "catalog: {problem}"),
                SourceProblem::Syntax { .. } => write!(f, "catalog: parsing {file}: {problem}"),
                SourceProblem::PluralNotString { .. }
                | SourceProblem::UnsupportedType { .. }
                | SourceProblem::DuplicateKey { .. } => write!(f, "catalog: {file}: {problem}"),
            },
            ProblemKind::CatalogNameNotUtf8 => write!(
                f,
                "catalog: {file} has a name that is not valid UTF-8, so a change to it will not mark the baked catalog stale"
            ),
        }
    }
}
