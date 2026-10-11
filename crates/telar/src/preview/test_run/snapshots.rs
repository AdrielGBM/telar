//! The snapshot half of a test run, in a build that can draw one.

use std::path::{Path, PathBuf};

use crate::AppConfig;
use crate::preview::snapshot::{
    SnapshotError, SnapshotMode, SnapshotPaths, SnapshotReport, SnapshotSummary, Snapshots,
    Tolerance,
};
use crate::preview::{Frame, MatrixCell, PlayError, PreviewEntry};

use super::options::{SnapshotSetting, TestOptions};
use super::report::{SnapshotCheck, TestReport, TestRun};
use super::snapshot_check::describe;

pub(super) struct SnapshotRun(Snapshots);

/// The snapshot run `options` asks for, with the declared faces made the only ones in the process.
pub(super) fn start(config: &AppConfig, options: &TestOptions) -> Result<SnapshotRun, String> {
    let (Some(package_dir), Some(target_dir)) = (&options.package_dir, &options.target_dir) else {
        return Err(String::from(
            "snapshots need to know where the package and the target directory are, which `cargo telar test` passes; run them through it",
        ));
    };
    let paths = SnapshotPaths::new(package_dir, target_dir);
    Snapshots::new(paths, mode(options.snapshots), config)
        .map(|snapshots| SnapshotRun(snapshots.with_tolerance(tolerance(options))))
        .map_err(|error| match error {
            SnapshotError::Fonts(renderer_text::fonts::ExclusiveFontsError::NoFaces) => {
                String::from(
                    "snapshots set text only in the faces the package declares, and it declares none: add one under `[[telar.fonts]]` in telar.toml, or pass --no-snapshots",
                )
            }
            error => error.to_string(),
        })
}

impl SnapshotRun {
    pub(super) fn check(
        &mut self,
        entry: &PreviewEntry,
        cell: Option<&MatrixCell>,
        opened: &Frame,
    ) -> SnapshotCheck {
        describe(self.0.check_frame(entry, cell, opened))
    }

    /// Records that the snapshot of `entry`, or of its `cell`, could not be taken because the preview did not build.
    pub(super) fn unbuilt(&mut self, entry: &PreviewEntry, cell: Option<&MatrixCell>, error: &str) {
        let id = cell.map_or(entry.id, |cell| &cell.id);
        let error = SnapshotError::Mount(PlayError::new(error));
        self.0
            .record_failure(id, entry.id, MatrixCell::owned_coordinates(cell), &error);
    }
}

/// Writes `report.json` under the target directory `options` names, with the snapshot report `run` made, or an empty one when there was none, and answers where.
pub(super) fn write_report(
    run: Option<SnapshotRun>,
    test: &TestRun,
    options: &TestOptions,
) -> Result<Option<PathBuf>, String> {
    let Some(target_dir) = &options.target_dir else {
        return Ok(None);
    };
    let package_dir = options.package_dir.as_deref().unwrap_or(Path::new(""));
    let paths = SnapshotPaths::new(package_dir, target_dir);
    let path = paths.report();
    let snapshots = match run {
        Some(run) => run.0.into_report(),
        None => SnapshotReport {
            version: SnapshotReport::VERSION,
            mode: mode(options.snapshots),
            goldens: paths.goldens,
            out: paths.out,
            tolerance: tolerance(options),
            summary: SnapshotSummary::default(),
            snapshots: Vec::new(),
        },
    };
    TestReport::new(snapshots, test.clone())
        .write(&path)
        .map(|()| Some(path.clone()))
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn mode(setting: SnapshotSetting) -> SnapshotMode {
    match setting {
        SnapshotSetting::Update => SnapshotMode::Update,
        SnapshotSetting::Compare | SnapshotSetting::Off => SnapshotMode::Compare,
    }
}

fn tolerance(options: &TestOptions) -> Tolerance {
    let default = Tolerance::default();
    Tolerance::new(
        options.channel_tolerance.unwrap_or(default.channel),
        options.max_differing_pixels.unwrap_or(default.max_pixels),
    )
}

#[cfg(test)]
#[path = "snapshots_test.rs"]
mod tests;
