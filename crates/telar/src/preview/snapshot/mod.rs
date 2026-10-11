//! Snapshots: each preview, and each cell of its matrix, compared against the two files kept for it.
//!
//! A snapshot is a picture and a text. The picture, `<id>.png`, is drawn on the CPU rasterizer and compared pixel by pixel within a [`Tolerance`]: it says whether anything *looks* different. The text, `<id>.draw.txt`, is the draw commands behind it one per line, compared exactly: it says *what* changed, as a line diff a reviewer reads — a padding change is a rect that moved, not a smear of pixels. Text sets in the faces the package declares and in nothing else the machine has, so both files are the same on every machine.
//!
//! The goldens live in `<package>/previews/__snapshots__/`, beside the sources they describe. A run writes nothing there unless it was asked to [update](SnapshotMode::Update); what does not match is written to `target/telar-snapshots/` instead — the picture and text captured, a diff picture, a line diff — with a [`SnapshotReport`] of the whole run as `report.json`.
//!
//! Taking and comparing snapshots, `Snapshots`, needs the `preview-snapshots` feature and the rasterizer it brings. Reading a report and [accepting](SnapshotReport::accept) what it found does not: a review panel writes the goldens through [`SnapshotPaths::write_goldens`], the same path an update run takes.
//!
//! ```ignore
//! let paths = SnapshotPaths::new(package_dir, target_dir);
//! let mut snapshots = Snapshots::new(paths, SnapshotMode::Compare, &config)?;
//! for entry in &entries {
//!     snapshots.check_entry(entry, &matrices);
//! }
//! let report = snapshots.finish()?;
//! ```

#[cfg(feature = "preview-snapshots")]
mod capture;
#[cfg(feature = "preview-snapshots")]
mod compare;
mod report;
#[cfg(feature = "preview-snapshots")]
mod run;
mod tolerance;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::PlayError;

pub use super::host::file_stem;
#[cfg(feature = "preview-snapshots")]
pub use capture::Snapshot;
pub use report::{
    DrawCheck, FileStatus, PngCheck, SnapshotReport, SnapshotResult, SnapshotStatus,
    SnapshotSummary,
};
#[cfg(feature = "preview-snapshots")]
pub use run::Snapshots;
pub use tolerance::Tolerance;

/// Whether a run only compares, or writes the goldens it finds missing or out of date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotMode {
    /// Compares, and writes what does not match beside the diffs, never over a golden.
    #[default]
    Compare,
    /// Writes both goldens of every snapshot that is missing one or no longer matches, and leaves a matching one untouched.
    Update,
}

/// Where a run reads its goldens and writes what it found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotPaths {
    /// `<package>/previews/__snapshots__`.
    pub goldens: PathBuf,
    /// `<target>/telar-snapshots`.
    pub out: PathBuf,
}

impl SnapshotPaths {
    /// The goldens of the package at `package_dir`, with what a run finds written under the Cargo target directory `target_dir`.
    pub fn new(package_dir: &Path, target_dir: &Path) -> Self {
        Self {
            goldens: package_dir.join("previews").join("__snapshots__"),
            out: target_dir.join(telar_project::protocol::TEST_OUT_DIR),
        }
    }

    /// The golden picture of the snapshot `id`.
    pub fn golden_png(&self, id: &str) -> PathBuf {
        self.goldens.join(format!("{}.png", file_stem(id)))
    }

    /// The golden draw text of the snapshot `id`.
    pub fn golden_draw(&self, id: &str) -> PathBuf {
        self.goldens.join(format!("{}.draw.txt", file_stem(id)))
    }

    pub fn report(&self) -> PathBuf {
        self.out.join(SnapshotReport::FILE_NAME)
    }

    /// Makes `png` and `draw` both goldens of the snapshot `id`: what an update run writes for each snapshot that is missing one or no longer matches, and what accepting a drift writes.
    pub fn write_goldens(&self, id: &str, png: &[u8], draw: &str) -> Result<(), SnapshotError> {
        write(&self.golden_png(id), png)?;
        write(&self.golden_draw(id), draw.as_bytes())
    }

    /// Removes what a run wrote for `id` beside the diffs, so the out directory holds findings that still stand alone.
    fn forget_findings(&self, id: &str) -> Result<(), SnapshotError> {
        for suffix in OUT_SUFFIXES {
            let path = self.out_file(id, suffix);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(SnapshotError::io(&path, error)),
            }
        }
        Ok(())
    }

    fn out_file(&self, id: &str, suffix: &str) -> PathBuf {
        self.out.join(format!("{}{suffix}", file_stem(id)))
    }
}

const OUT_SUFFIXES: [&str; 4] = [".png", ".draw.txt", ".diff.png", ".draw.diff"];

fn write(path: &Path, bytes: &[u8]) -> Result<(), SnapshotError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| SnapshotError::io(dir, error))?;
    }
    std::fs::write(path, bytes).map_err(|error| SnapshotError::io(path, error))
}

/// Why a snapshot could not be captured, compared, written or accepted.
#[non_exhaustive]
#[derive(Debug)]
pub enum SnapshotError {
    /// The declared faces could not be made the only ones text sets in.
    Fonts(renderer_text::fonts::ExclusiveFontsError),
    /// A matrix cell could not be expanded or set up.
    Cell(String),
    /// The preview did not mount, or failed as it did.
    Mount(PlayError),
    /// The rasterizer could not draw the frame or encode the picture.
    Render(String),
    /// Capturing panicked.
    Panicked,
    /// Two ids that differ only where a file name cannot, so they would share their goldens.
    SameFile {
        id: String,
        other: String,
    },
    /// The same id checked twice in one run.
    Repeated(String),
    /// A result with no captured files to make the goldens: one that matched, was written or could not be taken.
    NothingToAccept(String),
    /// An id the report holds no snapshot of.
    NotInReport(String),
    Io {
        path: PathBuf,
        error: io::Error,
    },
}

impl SnapshotError {
    fn io(path: &Path, error: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            error,
        }
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fonts(error) => write!(f, "snapshot fonts: {error}"),
            Self::Cell(error) => write!(f, "matrix cell: {error}"),
            Self::Mount(error) => write!(f, "the preview did not mount: {error}"),
            Self::Render(error) => write!(f, "the frame could not be drawn: {error}"),
            Self::Panicked => f.write_str("panicked while capturing the snapshot"),
            Self::SameFile { id, other } => write!(
                f,
                "`{id}` and `{other}` would share the same snapshot files; rename one"
            ),
            Self::Repeated(id) => write!(f, "`{id}` was snapshotted twice in one run"),
            Self::NothingToAccept(id) => {
                write!(f, "`{id}` has no captured snapshot to accept")
            }
            Self::NotInReport(id) => write!(f, "the report holds no snapshot of `{id}`"),
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Fonts(error) => Some(error),
            Self::Mount(error) => Some(error),
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}
