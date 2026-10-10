//! The snapshot half of a test run, in a build without `preview-snapshots`: it can take none and write no report.

use std::path::PathBuf;

use crate::AppConfig;
use crate::preview::{Frame, MatrixCell, PreviewEntry};

use super::options::TestOptions;
use super::report::{SnapshotCheck, TestRun};

pub(super) enum SnapshotRun {}

pub(super) fn start(_: &AppConfig, _: &TestOptions) -> Result<SnapshotRun, String> {
    Err(String::from(
        "this build cannot take snapshots: it was built without telar's `preview-snapshots` feature, which `cargo telar test` turns on",
    ))
}

impl SnapshotRun {
    pub(super) fn check(
        &mut self,
        _: &PreviewEntry,
        _: Option<&MatrixCell>,
        _: &Frame,
    ) -> SnapshotCheck {
        match *self {}
    }

    pub(super) fn unbuilt(&mut self, _: &PreviewEntry, _: Option<&MatrixCell>, _: &str) {
        match *self {}
    }
}

pub(super) fn write_report(
    _: Option<SnapshotRun>,
    _: &TestRun,
    options: &TestOptions,
) -> Result<Option<PathBuf>, String> {
    match options.target_dir {
        Some(_) => Err(String::from(
            "this build cannot write report.json: it was built without telar's `preview-snapshots` feature, which `cargo telar test` turns on",
        )),
        None => Ok(None),
    }
}
