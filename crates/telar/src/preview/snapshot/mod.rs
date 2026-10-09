//! Snapshots: each preview, and each cell of its matrix, compared against the two files kept for it.
//!
//! A snapshot is a picture and a text. The picture, `<id>.png`, is drawn on the CPU rasterizer and compared pixel by pixel within a [`Tolerance`]: it says whether anything *looks* different. The text, `<id>.draw.txt`, is the draw commands behind it one per line, compared exactly: it says *what* changed, as a line diff a reviewer reads — a padding change is a rect that moved, not a smear of pixels. Text sets in the faces the package declares and in nothing else the machine has, so both files are the same on every machine.
//!
//! The goldens live in `<package>/previews/__snapshots__/`, beside the sources they describe. A run writes nothing there unless it was asked to [update](SnapshotMode::Update); what does not match is written to `target/telar-snapshots/` instead — the picture and text captured, a diff picture, a line diff — with a [`SnapshotReport`] of the whole run as `report.json`.
//!
//! ```ignore
//! let paths = SnapshotPaths::new(package_dir, target_dir);
//! let mut snapshots = Snapshots::new(paths, SnapshotMode::Compare, &config)?;
//! for entry in &entries {
//!     snapshots.check_entry(entry, &matrices);
//! }
//! let report = snapshots.finish()?;
//! ```

mod capture;
mod compare;
mod report;

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::AppConfig;

use super::{Matrices, PlayError, PreviewEntry};

pub use super::host::file_stem;
pub use capture::Snapshot;
pub use compare::Tolerance;
pub use report::{
    DrawCheck, FileStatus, PngCheck, SnapshotReport, SnapshotResult, SnapshotStatus,
    SnapshotSummary,
};

use compare::{Image, line_diff};

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
            out: target_dir.join("telar-snapshots"),
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

    fn out_file(&self, id: &str, suffix: &str) -> PathBuf {
        self.out.join(format!("{}{suffix}", file_stem(id)))
    }
}

/// One snapshot run: every preview and matrix cell checked against its goldens, then a report.
///
/// Making one makes the faces `config` declares the only ones text sets in for the rest of the process, so it is made before anything measures text: a process that already loaded the platform's fonts cannot forget them, and is refused rather than allowed to set a snapshot in a face that only one machine has.
pub struct Snapshots {
    paths: SnapshotPaths,
    mode: SnapshotMode,
    tolerance: Tolerance,
    results: Vec<SnapshotResult>,
    /// Each file stem checked so far, with the id that took it: two ids that only differ where a file name cannot would overwrite each other's goldens.
    stems: HashMap<String, String>,
}

impl Snapshots {
    pub fn new(
        paths: SnapshotPaths,
        mode: SnapshotMode,
        config: &AppConfig,
    ) -> Result<Self, SnapshotError> {
        renderer_text::fonts::install_only(
            config.fonts.clone(),
            config.font_family.iter().cloned().collect(),
        )
        .map_err(SnapshotError::Fonts)?;
        Ok(Self {
            paths,
            mode,
            tolerance: Tolerance::default(),
            results: Vec::new(),
            stems: HashMap::new(),
        })
    }

    pub fn with_tolerance(self, tolerance: Tolerance) -> Self {
        Self { tolerance, ..self }
    }

    pub fn paths(&self) -> &SnapshotPaths {
        &self.paths
    }

    pub fn mode(&self) -> SnapshotMode {
        self.mode
    }

    pub fn tolerance(&self) -> Tolerance {
        self.tolerance
    }

    /// Every result so far, in the order checked.
    pub fn results(&self) -> &[SnapshotResult] {
        &self.results
    }

    /// Snapshots `entry` as it opens, then each cell of its matrix, and checks each against its goldens. A preview or cell that cannot be mounted or drawn is a failed result, a matrix that cannot be expanded is one failed result named `<preview-id>--matrix`, and the run goes on.
    pub fn check_entry(&mut self, entry: &PreviewEntry, matrices: &Matrices) -> &[SnapshotResult] {
        let start = self.results.len();
        self.check_captured(entry.id, entry.id, Vec::new(), || {
            capture::capture(entry, None)
        });
        if let Some(matrix) = entry.matrix {
            match matrix.cells(entry, matrices) {
                Ok(cells) => {
                    for cell in &cells {
                        let coordinates = cell
                            .coordinates
                            .iter()
                            .map(|(axis, value)| (axis.to_string(), value.clone()))
                            .collect();
                        self.check_captured(&cell.id, entry.id, coordinates, || {
                            capture::capture(entry, Some(cell))
                        });
                    }
                }
                Err(error) => {
                    let id = format!("{}--matrix", entry.id);
                    let error = SnapshotError::Cell(error.to_string());
                    self.record_failure(&id, entry.id, Vec::new(), &error);
                }
            }
        }
        &self.results[start..]
    }

    /// Checks `snapshot` against its goldens: compares it, or in an update run writes them where it does not match.
    pub fn check(&mut self, snapshot: &Snapshot) -> &SnapshotResult {
        let result = match self.claim_stem(&snapshot.id) {
            Ok(()) => self.compare(snapshot),
            Err(error) => failed(
                &snapshot.id,
                &snapshot.preview,
                snapshot.cell.clone(),
                &error,
            ),
        };
        self.push(result)
    }

    /// Records that the snapshot `id` of `preview` could not be captured.
    pub fn record_failure(
        &mut self,
        id: &str,
        preview: &str,
        cell: Vec<(String, String)>,
        error: &SnapshotError,
    ) -> &SnapshotResult {
        self.push(failed(id, preview, cell, error))
    }

    /// Writes the report of every snapshot checked to `report.json` under the out directory, and returns it.
    pub fn finish(self) -> Result<SnapshotReport, SnapshotError> {
        let report = SnapshotReport {
            version: SnapshotReport::VERSION,
            mode: self.mode,
            goldens: self.paths.goldens.clone(),
            out: self.paths.out.clone(),
            tolerance: self.tolerance,
            summary: SnapshotSummary::of(&self.results),
            snapshots: self.results,
        };
        let path = self.paths.report();
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| SnapshotError::io(&path, io::Error::other(error)))?;
        write(&path, json.as_bytes())?;
        Ok(report)
    }

    fn check_captured(
        &mut self,
        id: &str,
        preview: &str,
        cell: Vec<(String, String)>,
        capture: impl FnOnce() -> Result<Snapshot, SnapshotError>,
    ) {
        match catch_unwind(AssertUnwindSafe(capture)) {
            Ok(Ok(snapshot)) => {
                self.check(&snapshot);
            }
            Ok(Err(error)) => {
                self.record_failure(id, preview, cell, &error);
            }
            Err(_) => {
                self.record_failure(id, preview, cell, &SnapshotError::Panicked);
            }
        }
    }

    fn push(&mut self, result: SnapshotResult) -> &SnapshotResult {
        self.results.push(result);
        self.results.last().expect("just pushed")
    }

    fn claim_stem(&mut self, id: &str) -> Result<(), SnapshotError> {
        let stem = file_stem(id);
        match self.stems.get(&stem) {
            Some(other) if other != id => Err(SnapshotError::SameFile {
                id: id.to_string(),
                other: other.clone(),
            }),
            Some(_) => Err(SnapshotError::Repeated(id.to_string())),
            None => {
                self.stems.insert(stem, id.to_string());
                Ok(())
            }
        }
    }

    fn compare(&self, snapshot: &Snapshot) -> SnapshotResult {
        let outcome = self.forget_last_run(&snapshot.id).and_then(|()| {
            let goldens = Goldens::read(&self.paths, &snapshot.id)?;
            let found = Found::of(snapshot, &goldens, self.tolerance);
            match (self.mode, found.matches()) {
                (_, true) => Ok(self.matched(snapshot, &goldens, &found)),
                (SnapshotMode::Compare, false) => self.mismatched(snapshot, &goldens, &found),
                (SnapshotMode::Update, false) => self.update(snapshot, &goldens, &found),
            }
        });
        outcome.unwrap_or_else(|error| {
            failed(
                &snapshot.id,
                &snapshot.preview,
                snapshot.cell.clone(),
                &error,
            )
        })
    }

    /// Removes what an earlier run wrote for `id`, so the out directory holds this run's findings alone.
    fn forget_last_run(&self, id: &str) -> Result<(), SnapshotError> {
        for suffix in OUT_SUFFIXES {
            let path = self.paths.out_file(id, suffix);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(SnapshotError::io(&path, error)),
            }
        }
        Ok(())
    }

    fn matched(&self, snapshot: &Snapshot, goldens: &Goldens, found: &Found) -> SnapshotResult {
        self.result(snapshot, SnapshotStatus::Matched, goldens, found)
    }

    fn mismatched(
        &self,
        snapshot: &Snapshot,
        goldens: &Goldens,
        found: &Found,
    ) -> Result<SnapshotResult, SnapshotError> {
        let status = if goldens.png.is_none() || goldens.draw.is_none() {
            SnapshotStatus::Missing
        } else {
            SnapshotStatus::Changed
        };
        let mut result = self.result(snapshot, status, goldens, found);
        let id = &snapshot.id;
        let actual_png = self.paths.out_file(id, ".png");
        let actual_draw = self.paths.out_file(id, ".draw.txt");
        write(&actual_png, &snapshot.png)?;
        write(&actual_draw, snapshot.draw.as_bytes())?;
        if let Some(png) = &mut result.png {
            png.actual = Some(actual_png);
            if png.status == FileStatus::Changed
                && let Some(diff) = &found.pixels
            {
                let path = self.paths.out_file(id, ".diff.png");
                let encoded = diff
                    .diff
                    .encode_png()
                    .map_err(|error| SnapshotError::io(&path, io::Error::other(error)))?;
                write(&path, &encoded)?;
                png.diff = Some(path);
            }
        }
        if let Some(draw) = &mut result.draw {
            draw.actual = Some(actual_draw.clone());
            if draw.status == FileStatus::Changed
                && let Some(golden) = &goldens.draw
            {
                let path = self.paths.out_file(id, ".draw.diff");
                let diff = line_diff(
                    golden,
                    &snapshot.draw,
                    &draw.golden.display().to_string(),
                    &actual_draw.display().to_string(),
                );
                write(&path, diff.as_bytes())?;
                draw.diff = Some(path);
            }
        }
        Ok(result)
    }

    fn update(
        &self,
        snapshot: &Snapshot,
        goldens: &Goldens,
        found: &Found,
    ) -> Result<SnapshotResult, SnapshotError> {
        let mut result = self.result(snapshot, SnapshotStatus::Updated, goldens, found);
        write(&self.paths.golden_png(&snapshot.id), &snapshot.png)?;
        write(
            &self.paths.golden_draw(&snapshot.id),
            snapshot.draw.as_bytes(),
        )?;
        if let Some(png) = &mut result.png {
            png.status = FileStatus::Written;
        }
        if let Some(draw) = &mut result.draw {
            draw.status = FileStatus::Written;
        }
        Ok(result)
    }

    fn result(
        &self,
        snapshot: &Snapshot,
        status: SnapshotStatus,
        goldens: &Goldens,
        found: &Found,
    ) -> SnapshotResult {
        let image = &snapshot.image;
        let png = PngCheck {
            status: found.png,
            golden: self.paths.golden_png(&snapshot.id),
            actual: None,
            diff: None,
            size: [image.width, image.height],
            golden_size: goldens
                .png
                .as_ref()
                .map(|golden| [golden.width, golden.height]),
            differing_pixels: found.pixels.as_ref().map_or(0, |diff| diff.differing),
            max_channel_delta: found.pixels.as_ref().map_or(0, |diff| diff.max_delta),
        };
        let draw = DrawCheck {
            status: found.draw,
            golden: self.paths.golden_draw(&snapshot.id),
            actual: None,
            diff: None,
        };
        SnapshotResult {
            id: snapshot.id.clone(),
            preview: snapshot.preview.clone(),
            cell: snapshot.cell.clone(),
            status,
            png: Some(png),
            draw: Some(draw),
            error: None,
        }
    }
}

/// Makes what a run captured for `result` the goldens: what accepting a drift from a review panel does. Only a result whose captured files were written can be accepted — a changed or missing one.
pub fn accept(result: &SnapshotResult) -> Result<(), SnapshotError> {
    let (Some(png), Some(draw)) = (&result.png, &result.draw) else {
        return Err(SnapshotError::NothingToAccept(result.id.clone()));
    };
    let (Some(actual_png), Some(actual_draw)) = (&png.actual, &draw.actual) else {
        return Err(SnapshotError::NothingToAccept(result.id.clone()));
    };
    for (from, to) in [(actual_png, &png.golden), (actual_draw, &draw.golden)] {
        let bytes = std::fs::read(from).map_err(|error| SnapshotError::io(from, error))?;
        write(to, &bytes)?;
    }
    Ok(())
}

const OUT_SUFFIXES: [&str; 4] = [".png", ".draw.txt", ".diff.png", ".draw.diff"];

/// The goldens of one snapshot, where they exist.
struct Goldens {
    png: Option<Image>,
    draw: Option<String>,
}

impl Goldens {
    fn read(paths: &SnapshotPaths, id: &str) -> Result<Self, SnapshotError> {
        let png_path = paths.golden_png(id);
        let png = read_optional(&png_path)?
            .map(|bytes| {
                Image::decode(&bytes).map_err(|error| {
                    SnapshotError::io(&png_path, io::Error::new(io::ErrorKind::InvalidData, error))
                })
            })
            .transpose()?;
        let draw_path = paths.golden_draw(id);
        let draw = read_optional(&draw_path)?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|error| {
                    SnapshotError::io(
                        &draw_path,
                        io::Error::new(io::ErrorKind::InvalidData, error),
                    )
                })
            })
            .transpose()?;
        Ok(Self { png, draw })
    }
}

/// How a snapshot stands against its goldens, file by file.
struct Found {
    png: FileStatus,
    draw: FileStatus,
    pixels: Option<compare::PixelDiff>,
}

impl Found {
    fn of(snapshot: &Snapshot, goldens: &Goldens, tolerance: Tolerance) -> Self {
        let pixels = goldens
            .png
            .as_ref()
            .map(|golden| compare::compare(golden, &snapshot.image, tolerance));
        let png = match &pixels {
            None => FileStatus::Missing,
            Some(diff) if diff.within(tolerance) => FileStatus::Matched,
            Some(_) => FileStatus::Changed,
        };
        let draw = match &goldens.draw {
            None => FileStatus::Missing,
            Some(golden) if *golden == snapshot.draw => FileStatus::Matched,
            Some(_) => FileStatus::Changed,
        };
        Self { png, draw, pixels }
    }

    fn matches(&self) -> bool {
        self.png == FileStatus::Matched && self.draw == FileStatus::Matched
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, SnapshotError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(SnapshotError::io(path, error)),
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SnapshotError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| SnapshotError::io(dir, error))?;
    }
    std::fs::write(path, bytes).map_err(|error| SnapshotError::io(path, error))
}

fn failed(
    id: &str,
    preview: &str,
    cell: Vec<(String, String)>,
    error: &SnapshotError,
) -> SnapshotResult {
    SnapshotResult {
        id: id.to_string(),
        preview: preview.to_string(),
        cell,
        status: SnapshotStatus::Failed,
        png: None,
        draw: None,
        error: Some(error.to_string()),
    }
}

/// Why a snapshot could not be captured, compared or written.
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
    /// A result with no captured files to make the goldens.
    NothingToAccept(String),
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
