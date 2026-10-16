//! [`Snapshots`]: one run checking every preview and matrix cell against its goldens, in a build that can draw them.

use std::collections::HashMap;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::AppConfig;
use crate::preview::{Frame, Matrices, MatrixCell, PlayError, PreviewEntry};

use super::capture::{self, Snapshot};
use super::compare::{self, Image, line_diff};
use super::{
    DrawCheck, FileStatus, Moment, PngCheck, SnapshotError, SnapshotKey, SnapshotMode,
    SnapshotPaths, SnapshotReport, SnapshotResult, SnapshotStatus, SnapshotSummary, Tolerance,
    file_stem, write,
};

/// One snapshot run: every preview and matrix cell checked against its goldens, then a report.
///
/// Making one makes the faces `config` declares the only ones text sets in for the rest of the process, so it is made before anything measures text: a process that already loaded the platform's fonts cannot forget them, and is refused rather than allowed to set a snapshot in a face that only one machine has. It also shows every shortcut as a PC does, for the same reason.
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
        platform_core::set_keyboard_convention(platform_core::KeyboardConvention::Pc);
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

    /// Snapshots `entry`, then each cell of its matrix, at each moment it keeps, and checks each against its goldens. A preview or cell that cannot be mounted or drawn is a failed result, a matrix that cannot be expanded is one failed result named `<preview-id>--matrix`, and the run goes on.
    pub fn check_entry(&mut self, entry: &PreviewEntry, matrices: &Matrices) -> &[SnapshotResult] {
        let start = self.results.len();
        self.check_canvas(entry, None);
        if let Some(matrix) = entry.matrix {
            match matrix.cells(entry, matrices) {
                Ok(cells) => {
                    for cell in &cells {
                        self.check_canvas(entry, Some(cell));
                    }
                }
                Err(error) => {
                    let key = SnapshotKey {
                        preview: entry.id.to_string(),
                        ..SnapshotKey::named(format!("{}--matrix", entry.id))
                    };
                    self.record_failure(&key, &SnapshotError::Cell(error.to_string()));
                }
            }
        }
        &self.results[start..]
    }

    /// Checks each snapshot `keys` names against its goldens: the start against `opened`, the canvas as it opened, and the end against `ended`, the canvas as the play left it, or records why the play left none. What a run that already mounted the canvas and ran its play snapshots it with, instead of mounting it again.
    pub fn check_moments(
        &mut self,
        keys: &[SnapshotKey],
        opened: &Frame,
        ended: Result<&Frame, &PlayError>,
    ) -> &[SnapshotResult] {
        let start = self.results.len();
        for key in keys {
            let frame = match key.moment {
                Moment::Start => Ok(opened),
                Moment::End => ended.map_err(|error| SnapshotError::Play(error.clone())),
            };
            match frame.and_then(|frame| Snapshot::of_frame(key.clone(), frame)) {
                Ok(snapshot) => self.check(&snapshot),
                Err(error) => self.record_failure(key, &error),
            };
        }
        &self.results[start..]
    }

    /// Checks `snapshot` against its goldens: compares it, or in an update run writes them where it does not match.
    pub fn check(&mut self, snapshot: &Snapshot) -> &SnapshotResult {
        let result = match self.claim_stem(&snapshot.key.id) {
            Ok(()) => self.compare(snapshot),
            Err(error) => failed(&snapshot.key, &error),
        };
        self.push(result)
    }

    /// Records that the snapshot `key` names could not be captured.
    pub fn record_failure(&mut self, key: &SnapshotKey, error: &SnapshotError) -> &SnapshotResult {
        self.push(failed(key, error))
    }

    /// The report of every snapshot checked, without writing it: for a caller that writes it inside a report of its own.
    pub fn into_report(self) -> SnapshotReport {
        SnapshotReport {
            version: SnapshotReport::VERSION,
            mode: self.mode,
            goldens: self.paths.goldens,
            out: self.paths.out,
            tolerance: self.tolerance,
            summary: SnapshotSummary::of(&self.results),
            snapshots: self.results,
        }
    }

    /// Writes the report of every snapshot checked to `report.json` under the out directory, and returns it.
    pub fn finish(self) -> Result<SnapshotReport, SnapshotError> {
        let path = self.paths.report();
        let report = self.into_report();
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| SnapshotError::io(&path, io::Error::other(error)))?;
        write(&path, json.as_bytes())?;
        Ok(report)
    }

    fn check_canvas(&mut self, entry: &PreviewEntry, cell: Option<&MatrixCell>) {
        let keys = SnapshotKey::of(entry, cell);
        match catch_unwind(AssertUnwindSafe(|| capture::open_and_play(entry, cell))) {
            Ok(Ok((opened, ended))) => {
                self.check_moments(&keys, &opened, ended.as_ref());
            }
            Ok(Err(error)) => self.record_failures(&keys, &error),
            Err(_) => self.record_failures(&keys, &SnapshotError::Panicked),
        }
    }

    fn record_failures(&mut self, keys: &[SnapshotKey], error: &SnapshotError) {
        for key in keys {
            self.record_failure(key, error);
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
        let outcome = self.paths.forget_findings(&snapshot.key.id).and_then(|()| {
            let goldens = Goldens::read(&self.paths, &snapshot.key.id)?;
            let found = Found::of(snapshot, &goldens, self.tolerance);
            match (self.mode, found.matches()) {
                (_, true) => Ok(self.matched(snapshot, &goldens, &found)),
                (SnapshotMode::Compare, false) => self.mismatched(snapshot, &goldens, &found),
                (SnapshotMode::Update, false) => self.update(snapshot, &goldens, &found),
            }
        });
        outcome.unwrap_or_else(|error| failed(&snapshot.key, &error))
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
        let id = &snapshot.key.id;
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
        self.paths
            .write_goldens(&snapshot.key.id, &snapshot.png, &snapshot.draw)?;
        result.mark_written();
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
            golden: self.paths.golden_png(&snapshot.key.id),
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
            golden: self.paths.golden_draw(&snapshot.key.id),
            actual: None,
            diff: None,
        };
        SnapshotResult {
            png: Some(png),
            draw: Some(draw),
            ..SnapshotResult::of(&snapshot.key, status)
        }
    }
}

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

fn failed(key: &SnapshotKey, error: &SnapshotError) -> SnapshotResult {
    SnapshotResult {
        error: Some(error.to_string()),
        ..SnapshotResult::of(key, SnapshotStatus::Failed)
    }
}
