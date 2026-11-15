//! What a test run found, preview by preview and cell by cell, as `report.json` carries it beside the snapshot report.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::options::{A11yLevel, SnapshotSetting};
use super::snapshot_check::describe;
use crate::preview::a11y;
use crate::preview::snapshot::{SnapshotError, SnapshotReport};

/// A whole test run: what it was asked to do, each preview and cell it checked, and what failed outside any of them.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TestRun {
    /// The glob the run was narrowed to, if any.
    pub filter: Option<String>,
    /// How loudly accessibility findings were reported in this run.
    pub a11y: A11yLevel,
    /// What the run did with snapshots.
    pub snapshots: SnapshotSetting,
    /// The run's results, counted.
    pub summary: TestSummary,
    /// What failed the run without belonging to one preview: two previews under one id, snapshots that could not be set up, a filter that matched nothing.
    pub errors: Vec<String>,
    /// One report per preview the run checked.
    pub previews: Vec<PreviewReport>,
}

impl TestRun {
    /// Whether nothing failed: no preview, cell or run-level error.
    pub fn passed(&self) -> bool {
        self.summary.failed == 0
    }

    pub(super) fn summarize(&mut self) {
        let cells = self.previews.iter().flat_map(|preview| &preview.cells);
        let (mut passed, mut failed, mut warned) = (0, 0, 0);
        for cell in cells {
            match cell.passed {
                true => passed += 1,
                false => failed += 1,
            }
            if cell.a11y.status == CheckStatus::Warned {
                warned += 1;
            }
        }
        self.summary = TestSummary {
            previews: self.previews.len(),
            checked: passed + failed,
            passed,
            failed: failed + self.errors.len(),
            warned,
        };
    }
}

/// How the run came out, counted.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSummary {
    /// How many previews the run covered.
    pub previews: usize,
    /// Previews and matrix cells checked, each counted once.
    pub checked: usize,
    /// Previews and cells that passed, with or without warnings.
    pub passed: usize,
    /// Previews and cells that failed, and each of [`TestRun::errors`].
    pub failed: usize,
    /// Previews and cells that passed or failed with accessibility warnings.
    pub warned: usize,
}

/// One preview: where it is written and how it and each cell of its matrix came out, the preview itself first.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviewReport {
    /// The preview's id, which its cells' ids start with.
    pub id: String,
    /// The preview's title, as a panel lists it.
    pub title: String,
    /// The component the preview shows.
    pub component: String,
    /// The preview's name under its component.
    pub name: String,
    /// The file the preview is written in, empty when unknown.
    pub file: String,
    /// Its 1-based line there, 0 when unknown.
    pub line: u32,
    /// The preview as it opens first, then each cell of its matrix.
    pub cells: Vec<CellReport>,
}

/// The preview as it opens, or one cell of its matrix, through each check in the order they ran.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellReport {
    /// The preview's id, or the cell's: `<preview-id>--<axis>=<value>…`.
    pub id: String,
    /// The cell's axis and value pairs, in axis order; empty for the preview itself.
    pub cell: Vec<(String, String)>,
    /// Whether no check of this cell failed.
    pub passed: bool,
    /// Building the preview and laying it out.
    pub build: BuildCheck,
    /// Running the preview's play.
    pub play: PlayCheck,
    /// The accessibility checks.
    pub a11y: A11yCheck,
    /// The snapshot comparison.
    pub snapshot: SnapshotCheck,
    /// Unmounting the preview.
    pub teardown: TeardownCheck,
}

impl CellReport {
    #[cfg(not(target_os = "android"))]
    pub(super) fn new(id: String, cell: Vec<(String, String)>) -> Self {
        Self {
            id,
            cell,
            ..Self::default()
        }
    }

    pub(super) fn settle(mut self) -> Self {
        self.passed = [
            self.build.status,
            self.play.status,
            self.a11y.status,
            self.snapshot.status,
            self.teardown.status,
        ]
        .iter()
        .all(|status| *status != CheckStatus::Failed);
        self
    }

    /// What a run prints under the cell's line: each check's error and each accessibility finding, one per entry, prefixed with the check it came from.
    pub fn details(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(error) = &self.build.error {
            lines.push(format!("build: {error}"));
        }
        if let Some(error) = &self.play.error {
            lines.push(format!("play: {error}"));
        }
        if let Some(error) = &self.a11y.error {
            lines.push(format!("a11y: {error}"));
        }
        lines.extend(
            self.a11y
                .violations
                .iter()
                .map(|finding| format!("a11y: {}", finding.line())),
        );
        if self.snapshot.status == CheckStatus::Failed
            && let Some(detail) = &self.snapshot.detail
        {
            lines.push(format!("snapshot: {detail}"));
        }
        if let Some(error) = &self.teardown.error {
            lines.push(format!("teardown: {error}"));
        }
        lines
    }
}

/// How one check came out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// The check ran and found nothing to fail on.
    Passed,
    /// Passed with findings worth reading: accessibility warnings, or errors under `--a11y warn`.
    Warned,
    /// The check ran and failed the cell.
    Failed,
    /// Not run: turned off, nothing to run, or an earlier check failed.
    #[default]
    Skipped,
}

/// Building the preview and laying it out on its canvas.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCheck {
    /// How the build came out.
    pub status: CheckStatus,
    /// How many draw commands the canvas opened on.
    pub draw_commands: usize,
    /// Why the build failed.
    pub error: Option<String>,
}

/// The preview's play function.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayCheck {
    /// How the play came out.
    pub status: CheckStatus,
    /// Each step of the play, in the order run.
    pub steps: Vec<PlayStepReport>,
    /// Why the play stopped.
    pub error: Option<String>,
}

/// One step of a play.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayStepReport {
    /// What was done, as a panel lists it: `click button "Save"`.
    pub action: String,
    /// Why the step failed, when it did.
    pub error: Option<String>,
}

/// The accessibility checks, on the canvas as the play left it.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct A11yCheck {
    /// How the accessibility checks came out.
    pub status: CheckStatus,
    /// Every finding, errors and warnings alike.
    pub violations: Vec<A11yFinding>,
    /// Why the checks could not run.
    pub error: Option<String>,
}

/// One accessibility finding.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct A11yFinding {
    /// The rule's id, such as `contrast`.
    pub rule: String,
    /// `error` or `warning`.
    pub severity: String,
    /// What the rule found, in words.
    pub message: String,
    /// Where, as `[x, y, width, height]` in the canvas's logical px.
    pub rect: Option<[f32; 4]>,
}

impl From<&a11y::Violation> for A11yFinding {
    fn from(violation: &a11y::Violation) -> Self {
        Self {
            rule: violation.rule.id().to_string(),
            severity: violation.severity.as_str().to_string(),
            message: violation.message.clone(),
            rect: violation
                .rect
                .map(|rect| [rect.x, rect.y, rect.width, rect.height]),
        }
    }
}

impl A11yFinding {
    /// The finding as a run prints it.
    pub fn line(&self) -> String {
        format!("{:<7}  {:<16}  {}", self.severity, self.rule, self.message)
    }
}

/// The snapshots of the preview or the cell: of its canvas as its play left it, as it opened, or both, as the preview asks. The full result of each, with the paths of its goldens and diffs, is in the snapshot report beside this one.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCheck {
    /// How the snapshot comparison came out.
    pub status: CheckStatus,
    /// What the comparison found, in words.
    pub detail: Option<String>,
}

/// Unmounting the preview once every other check has run: a panic there fails the cell like any other check.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeardownCheck {
    /// How the teardown came out.
    pub status: CheckStatus,
    /// Why the teardown failed, when it did.
    pub error: Option<String>,
}

/// `report.json`: the snapshot report, which a review panel reads as it is, with the test run beside it under `test`.
///
/// ```json
/// {
///   "version": 2, "mode": "compare", "goldens": "…", "out": "…", "tolerance": { … }, "summary": { … }, "snapshots": [ … ],
///   "test": {
///     "filter": null, "a11y": "warn", "snapshots": "compare",
///     "summary": { "previews": 1, "checked": 2, "passed": 1, "failed": 1, "warned": 0 },
///     "errors": [],
///     "previews": [{
///       "id": "catalogue--button--primary", "title": "Button", "component": "button", "name": "Primary", "file": "…/button.rsx", "line": 12,
///       "cells": [{
///         "id": "catalogue--button--primary--mode=dark", "cell": [["mode", "dark"]], "passed": false,
///         "build": { "status": "passed", "draw_commands": 14, "error": null },
///         "play": { "status": "failed", "steps": [{ "action": "click button \"Save\"", "error": "found no button \"Save\"" }], "error": "found no button \"Save\"" },
///         "a11y": { "status": "passed", "violations": [], "error": null },
///         "snapshot": { "status": "passed", "detail": "matched" },
///         "teardown": { "status": "passed", "error": null }
///       }]
///     }]
///   }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestReport {
    #[serde(flatten)]
    /// The snapshot report, whose fields sit at the top level of `report.json`.
    pub snapshots: SnapshotReport,
    /// The test run, under `test` in `report.json`.
    pub test: TestRun,
}

impl TestReport {
    /// A report of the snapshots a run took together with the test run that took them.
    pub fn new(snapshots: SnapshotReport, test: TestRun) -> Self {
        Self { snapshots, test }
    }

    /// Reads a report a run wrote, or says why it cannot be read.
    pub fn read(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(io::Error::other)
    }

    /// Writes the report to `path`, replacing what is there in one step.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        // Atomically, so a reader watching the report, as the workshop does, never reads one half written.
        fs_atomic::write_atomically(path, json.as_bytes())
    }

    /// [`SnapshotReport::accept`], with the cell the snapshot `id` is of reported as a run that wrote its goldens reports it, and the run counted again.
    pub fn accept(&mut self, id: &str) -> Result<(), SnapshotError> {
        let subject = self.snapshots.accept(id)?.subject.clone();
        let check = describe(
            self.snapshots
                .snapshots
                .iter()
                .filter(|result| result.subject == subject),
        );
        let cells = self
            .test
            .previews
            .iter_mut()
            .flat_map(|preview| preview.cells.iter_mut())
            .filter(|cell| cell.id == subject);
        for cell in cells {
            cell.snapshot = check.clone();
            *cell = std::mem::take(cell).settle();
        }
        self.test.summarize();
        Ok(())
    }
}

#[cfg(test)]
#[path = "report.test.rs"]
mod tests;
