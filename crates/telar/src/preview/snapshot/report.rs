//! `report.json`: what a snapshot run found, for whatever shows it afterwards — a workshop panel accepting a drift, a test runner's summary, a CI job uploading the diffs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{SnapshotError, SnapshotMode, SnapshotPaths, Tolerance};

/// The `report.json` a run writes beside its diffs. [`SnapshotReport::VERSION`] moves whenever a field changes meaning or goes away, so a reader can refuse a report it does not understand.
///
/// ```json
/// {
///   "version": 1,
///   "mode": "compare",
///   "goldens": "/repo/apps/catalogue/previews/__snapshots__",
///   "out": "/repo/target/telar-snapshots",
///   "tolerance": { "channel": 2, "max_pixels": 0 },
///   "summary": { "total": 2, "matched": 1, "updated": 0, "failed": 1 },
///   "snapshots": [
///     {
///       "id": "catalogue--button--primary--mode=dark",
///       "preview": "catalogue--button--primary",
///       "cell": [["mode", "dark"]],
///       "status": "changed",
///       "png": {
///         "status": "changed",
///         "golden": "…/__snapshots__/catalogue--button--primary--mode=dark.png",
///         "actual": "…/telar-snapshots/catalogue--button--primary--mode=dark.png",
///         "diff": "…/telar-snapshots/catalogue--button--primary--mode=dark.diff.png",
///         "size": [800, 600],
///         "golden_size": [800, 600],
///         "differing_pixels": 312,
///         "max_channel_delta": 64
///       },
///       "draw": {
///         "status": "changed",
///         "golden": "…/__snapshots__/catalogue--button--primary--mode=dark.draw.txt",
///         "actual": "…/telar-snapshots/catalogue--button--primary--mode=dark.draw.txt",
///         "diff": "…/telar-snapshots/catalogue--button--primary--mode=dark.draw.diff"
///       },
///       "error": null
///     }
///   ]
/// }
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotReport {
    pub version: u32,
    pub mode: SnapshotMode,
    /// Where the goldens live: `<package>/previews/__snapshots__`.
    pub goldens: PathBuf,
    /// Where the actual pictures, the diffs and this report are written: `target/telar-snapshots`.
    pub out: PathBuf,
    pub tolerance: Tolerance,
    pub summary: SnapshotSummary,
    /// One per snapshot, in the order they were checked.
    pub snapshots: Vec<SnapshotResult>,
}

impl SnapshotReport {
    pub const VERSION: u32 = 1;

    /// The file name a run gives its report, inside [`out`](Self::out).
    pub const FILE_NAME: &'static str = telar_project::protocol::TEST_REPORT_FILE;

    /// A report a run wrote, or why it cannot be read.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(std::io::Error::other)
    }

    /// Whether every snapshot matched or was written.
    pub fn passed(&self) -> bool {
        self.summary.failed == 0
    }

    /// Where the run read its goldens and wrote what it found.
    pub fn paths(&self) -> SnapshotPaths {
        SnapshotPaths {
            goldens: self.goldens.clone(),
            out: self.out.clone(),
        }
    }

    /// Makes what the run captured for the snapshot `id` its goldens, as an update run would have written them, and records it as written: what accepting a drift from a review panel does. Only a changed or missing snapshot has captured files to accept; they are removed once they are the goldens.
    pub fn accept(&mut self, id: &str) -> Result<&SnapshotResult, SnapshotError> {
        let at = self
            .snapshots
            .iter()
            .position(|result| result.id == id)
            .ok_or_else(|| SnapshotError::NotInReport(id.to_string()))?;
        let paths = self.paths();
        let (png, draw) = self.snapshots[at].captured()?;
        paths.write_goldens(id, &png, &draw)?;
        paths.forget_findings(id)?;
        self.snapshots[at].mark_written();
        self.summary = SnapshotSummary::of(&self.snapshots);
        Ok(&self.snapshots[at])
    }
}

/// How many snapshots ended in each state.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub total: usize,
    pub matched: usize,
    pub updated: usize,
    /// Changed, missing or failed: what fails a test run.
    pub failed: usize,
}

impl SnapshotSummary {
    pub(crate) fn of(results: &[SnapshotResult]) -> Self {
        let count = |status: SnapshotStatus| results.iter().filter(|r| r.status == status).count();
        Self {
            total: results.len(),
            matched: count(SnapshotStatus::Matched),
            updated: count(SnapshotStatus::Updated),
            failed: results.iter().filter(|r| r.status.is_failure()).count(),
        }
    }
}

/// What one snapshot — a preview, or one cell of its matrix — came to.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotResult {
    /// The preview's id, or the cell's: what both golden files are named by.
    pub id: String,
    /// The id of the preview the snapshot is of.
    pub preview: String,
    /// The matrix cell's axis and value pairs, in axis order; empty for the preview itself.
    pub cell: Vec<(String, String)>,
    pub status: SnapshotStatus,
    /// `None` when nothing was captured to compare.
    pub png: Option<PngCheck>,
    pub draw: Option<DrawCheck>,
    /// Why nothing was captured, or why a file could not be read or written.
    pub error: Option<String>,
}

impl SnapshotResult {
    /// Whether [`SnapshotReport::accept`] can make what was captured the goldens: the snapshot is changed or missing, and the run wrote both of its captured files.
    pub fn acceptable(&self) -> bool {
        matches!(
            self.status,
            SnapshotStatus::Changed | SnapshotStatus::Missing
        ) && self.actual_files().is_some()
    }

    fn actual_files(&self) -> Option<(&PathBuf, &PathBuf)> {
        let png = self.png.as_ref()?.actual.as_ref()?;
        let draw = self.draw.as_ref()?.actual.as_ref()?;
        Some((png, draw))
    }

    /// The picture and the draw text the run captured, read back from beside the diffs.
    fn captured(&self) -> Result<(Vec<u8>, String), SnapshotError> {
        let (png, draw) = self
            .actual_files()
            .filter(|_| self.acceptable())
            .ok_or_else(|| SnapshotError::NothingToAccept(self.id.clone()))?;
        let png = std::fs::read(png).map_err(|error| SnapshotError::io(png, error))?;
        let draw = std::fs::read_to_string(draw).map_err(|error| SnapshotError::io(draw, error))?;
        Ok((png, draw))
    }

    /// Records both goldens as written from what was captured, as an update run records each it writes.
    pub(super) fn mark_written(&mut self) {
        self.status = SnapshotStatus::Updated;
        if let Some(png) = &mut self.png {
            png.status = FileStatus::Written;
            png.actual = None;
            png.diff = None;
        }
        if let Some(draw) = &mut self.draw {
            draw.status = FileStatus::Written;
            draw.actual = None;
            draw.diff = None;
        }
    }
}

/// Where a snapshot stands against its goldens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStatus {
    /// The picture is within the tolerance and the draw text is the same: nothing was written.
    Matched,
    /// An update run wrote both goldens, which were missing or no longer matched.
    Updated,
    /// The picture drifted past the tolerance, or the draw text changed.
    Changed,
    /// A golden is not there yet.
    Missing,
    /// Nothing could be captured or compared: see [`SnapshotResult::error`].
    Failed,
}

impl SnapshotStatus {
    /// Whether this status fails a test run.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Changed | Self::Missing | Self::Failed)
    }
}

/// Where one of a snapshot's two files stands against its golden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Matched,
    Changed,
    Missing,
    /// An update run wrote it.
    Written,
}

/// The picture against its golden.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PngCheck {
    pub status: FileStatus,
    pub golden: PathBuf,
    /// The picture captured, written beside the diffs whenever the snapshot does not match, so accepting it is a copy.
    pub actual: Option<PathBuf>,
    /// Where it differs, in a loud colour over a faded copy: only when it drifted past the tolerance.
    pub diff: Option<PathBuf>,
    /// Width and height of the picture captured, in pixels.
    pub size: [u32; 2],
    /// Width and height of the golden, where there is one.
    pub golden_size: Option<[u32; 2]>,
    /// Pixels past the per-channel tolerance, counting each one only one of the two pictures has.
    pub differing_pixels: u64,
    /// The most any channel moved, 0–255.
    pub max_channel_delta: u8,
}

/// The draw text against its golden.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawCheck {
    pub status: FileStatus,
    pub golden: PathBuf,
    /// The text captured, written beside the diffs whenever the snapshot does not match.
    pub actual: Option<PathBuf>,
    /// A unified diff of the golden against the text captured: only when they differ.
    pub diff: Option<PathBuf>,
}
